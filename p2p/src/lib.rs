// Copyright (c) 2021-2022 RBB S.r.l
// opensource@mintlayer.org
// SPDX-License-Identifier: MIT
// Licensed under the MIT License;
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://github.com/mintlayer/mintlayer-core/blob/master/LICENSE
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

pub mod config;
pub mod error;
pub mod event;
pub mod interface;
pub mod message;
pub mod net;
pub mod peer_manager;
pub mod protocol;
pub mod rpc;
pub mod sync;
pub mod types;
pub mod utils;

#[cfg(feature = "testing_utils")]
pub mod testing_utils;

use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use tokio::{sync::mpsc, task::JoinHandle};

use interface::p2p_interface::P2pInterface;
use net::default_backend::transport::{
    NoiseSocks5Transport, Socks5TransportSocket, TcpTransportSocket,
};
use peer_manager::peerdb::storage::PeerDbStorage;
use subsystem::{CallRequest, ShutdownRequest};

use ::utils::ensure;
use chainstate::chainstate_interface;
use common::{
    chain::{config::ChainType, ChainConfig},
    time_getter::TimeGetter,
};
use logging::log;
use mempool::MempoolHandle;

use crate::{
    config::P2pConfig,
    error::{ConversionError, P2pError},
    event::PeerManagerEvent,
    interface::p2p_interface::P2pSubsystemInterface,
    message::{BlockListRequest, SyncMessage},
    net::{
        default_backend::{
            transport::{NoiseEncryptionAdapter, NoiseTcpTransport},
            DefaultNetworkingService,
        },
        ConnectivityService, MessagingService, NetworkingService, SyncingEventReceiver,
    },
    utils::oneshot_nofail,
};

/// Result type with P2P errors
pub type Result<T> = core::result::Result<T, P2pError>;

struct P2p<T: NetworkingService> {
    /// A sender for the peer manager events.
    pub tx_peer_manager: mpsc::UnboundedSender<PeerManagerEvent<T>>,
    mempool_handle: MempoolHandle,
    messaging_handle: T::MessagingHandle,

    shutdown: Arc<AtomicBool>,

    backend_task: JoinHandle<()>,
    peer_manager_task: JoinHandle<()>,
    sync_manager_task: JoinHandle<()>,
}

impl<T> P2p<T>
where
    T: 'static + NetworkingService + Send,
    T::ConnectivityHandle: ConnectivityService<T>,
    T::MessagingHandle: MessagingService,
    T::SyncingEventReceiver: SyncingEventReceiver,
{
    /// Start the P2P subsystem
    ///
    /// This function starts the networking backend and individual manager objects.
    #[allow(clippy::too_many_arguments)]
    pub async fn new<S: PeerDbStorage + 'static>(
        transport: T::Transport,
        bind_addresses: Vec<T::Address>,
        chain_config: Arc<ChainConfig>,
        p2p_config: Arc<P2pConfig>,
        chainstate_handle: subsystem::Handle<Box<dyn chainstate_interface::ChainstateInterface>>,
        mempool_handle: MempoolHandle,
        time_getter: TimeGetter,
        peerdb_storage: S,
    ) -> Result<Self> {
        let shutdown = Arc::new(AtomicBool::new(false));

        let (conn, messaging_handle, sync_event_receiver, backend_task) = T::start(
            transport,
            bind_addresses,
            Arc::clone(&chain_config),
            Arc::clone(&p2p_config),
            Arc::clone(&shutdown),
        )
        .await?;

        // P2P creates its components (such as PeerManager, sync, pubsub, etc) and makes
        // communications with them in two possible ways:
        //
        // 1. Fire-and-forget
        // 2. Request and wait for response
        //
        // The difference between these types is that enums that contain the events *can* have
        // a `oneshot::channel` object that must be used to send the response.
        let (tx_peer_manager, rx_peer_manager) = mpsc::unbounded_channel();

        let mut peer_manager = peer_manager::PeerManager::<T, _>::new(
            Arc::clone(&chain_config),
            Arc::clone(&p2p_config),
            conn,
            rx_peer_manager,
            time_getter.clone(),
            peerdb_storage,
            Arc::clone(&shutdown),
        )?;
        let shutdown_ = Arc::clone(&shutdown);
        let peer_manager_task = tokio::spawn(async move {
            match peer_manager.run().await {
                Ok(_) => {}
                // The channel can be closed during the shutdown process.
                Err(P2pError::ChannelClosed) if shutdown_.load(Ordering::Acquire) => {
                    log::info!("Peer manager is shut down");
                }
                Err(e) => {
                    shutdown_.store(true, Ordering::Release);
                    log::error!("Peer manager failed: {e:?}");
                }
            }
        });

        let sync_manager_task = {
            let chainstate_handle = chainstate_handle.clone();
            let tx_peer_manager = tx_peer_manager.clone();
            let chain_config = Arc::clone(&chain_config);
            let mempool_handle_ = mempool_handle.clone();
            let messaging_handle_ = messaging_handle.clone();
            let shutdown_ = Arc::clone(&shutdown);

            tokio::spawn(async move {
                match sync::BlockSyncManager::<T>::new(
                    chain_config,
                    p2p_config,
                    messaging_handle_,
                    sync_event_receiver,
                    chainstate_handle,
                    mempool_handle_,
                    tx_peer_manager,
                    time_getter,
                    Arc::clone(&shutdown_),
                )
                .run()
                .await
                {
                    Ok(_) => unreachable!(),
                    // The channel can be closed during the shutdown process.
                    Err(P2pError::ChannelClosed) if shutdown_.load(Ordering::Acquire) => {
                        log::info!("Sync manager is shut down");
                    }
                    Err(e) => {
                        shutdown_.store(true, Ordering::Release);
                        log::error!("Sync manager failed: {e:?}");
                    }
                }
            })
        };

        Ok(Self {
            tx_peer_manager,
            mempool_handle,
            messaging_handle,
            shutdown,
            backend_task,
            peer_manager_task,
            sync_manager_task,
        })
    }

    async fn run(mut self, mut call: CallRequest<dyn P2pInterface>, mut shutdown: ShutdownRequest) {
        log::trace!("Entering p2p main loop");
        loop {
            tokio::select! {
                () = shutdown.recv() => {
                    self.shutdown().await;
                    break;
                },
                call = call.recv() => call(&mut self).await,
            }
        }
    }

    async fn shutdown(mut self) {
        self.shutdown.store(true, Ordering::Release);

        // Send messages to trigger the shutdown processing. This is only needed in a rare case
        // of no activity in the p2p parts which can happen in the tests.
        let (sender, receiver) = oneshot_nofail::channel();
        drop(receiver);
        let _ = self.tx_peer_manager.send(PeerManagerEvent::GetPeerCount(sender));

        let _ = self.messaging_handle.broadcast_message(SyncMessage::BlockListRequest(
            BlockListRequest::new(Vec::new()),
        ));

        // Wait for the tasks to shut dow.
        futures::future::join_all(
            [self.backend_task, self.peer_manager_task, self.sync_manager_task].into_iter(),
        )
        .await;
    }
}

impl subsystem::Subsystem for Box<dyn P2pInterface> {}

pub type P2pHandle = subsystem::Handle<dyn P2pInterface>;

pub type P2pNetworkingService = DefaultNetworkingService<NoiseTcpTransport>;
pub type P2pNetworkingServiceSocks5Proxy = DefaultNetworkingService<NoiseSocks5Transport>;
pub type P2pNetworkingServiceUnencrypted = DefaultNetworkingService<TcpTransportSocket>;

pub fn make_p2p_transport() -> NoiseTcpTransport {
    let stream_adapter = NoiseEncryptionAdapter::gen_new();
    let base_transport = TcpTransportSocket::new();
    NoiseTcpTransport::new(stream_adapter, base_transport)
}

pub fn make_p2p_transport_socks5_proxy(proxy: &str) -> NoiseSocks5Transport {
    let stream_adapter = NoiseEncryptionAdapter::gen_new();
    let base_transport = Socks5TransportSocket::new(proxy);
    NoiseSocks5Transport::new(stream_adapter, base_transport)
}

pub fn make_p2p_transport_unencrypted() -> TcpTransportSocket {
    TcpTransportSocket::new()
}

fn get_p2p_bind_addresses<S: AsRef<str>>(
    bind_addresses: &[S],
    p2p_port: u16,
    proxy_used: bool,
) -> Result<Vec<SocketAddr>> {
    if !bind_addresses.is_empty() {
        bind_addresses
            .iter()
            .map(|address| {
                address
                    .as_ref()
                    .parse::<<P2pNetworkingService as NetworkingService>::Address>()
                    .map_err(|_| {
                        P2pError::ConversionError(ConversionError::InvalidAddress(
                            address.as_ref().to_owned(),
                        ))
                    })
            })
            .collect::<Result<Vec<_>>>()
    } else if !proxy_used {
        // Bind to default addresses if none are specified by the user
        Ok(vec![
            SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), p2p_port),
            SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), p2p_port),
        ])
    } else {
        Ok(Vec::new())
    }
}

struct P2pInit<S: PeerDbStorage + 'static> {
    chain_config: Arc<ChainConfig>,
    p2p_config: Arc<P2pConfig>,
    chainstate_handle: subsystem::Handle<Box<dyn chainstate_interface::ChainstateInterface>>,
    mempool_handle: MempoolHandle,
    time_getter: TimeGetter,
    peerdb_storage: S,
    bind_addresses: Vec<SocketAddr>,
}

pub fn make_p2p<S: PeerDbStorage + 'static>(
    chain_config: Arc<ChainConfig>,
    p2p_config: Arc<P2pConfig>,
    chainstate_handle: subsystem::Handle<Box<dyn chainstate_interface::ChainstateInterface>>,
    mempool_handle: MempoolHandle,
    time_getter: TimeGetter,
    peerdb_storage: S,
) -> Result<impl P2pSubsystemInterface> {
    // Check that the `testing_utils` feature is not enabled when the node is built
    // (`testing_utils` turns on time mocking in tokio and also calls `setrlimit` when loaded).
    // TODO: FIXME:
    //assert!(cfg!(not(feature = "testing_utils")));

    // Perform some early checks to prevent a failure in the run method.
    let bind_addresses = get_p2p_bind_addresses(
        &p2p_config.bind_addresses,
        chain_config.p2p_port(),
        p2p_config.socks5_proxy.is_some(),
    )?;

    if let Some(true) = p2p_config.disable_noise {
        ensure!(
            *chain_config.chain_type() == ChainType::Regtest,
            P2pError::InvalidConfigurationValue(
                "P2P encryption can only be disabled on the regtest network".to_owned()
            )
        );
        ensure!(
            p2p_config.socks5_proxy.is_none(),
            P2pError::InvalidConfigurationValue(
                "SOCKS5 proxy support is not implemented for unencrypted".to_owned()
            )
        );
    }

    Ok(P2pInit {
        chain_config,
        p2p_config,
        chainstate_handle,
        mempool_handle,
        time_getter,
        peerdb_storage,
        bind_addresses,
    })
}

async fn run_p2p<S: PeerDbStorage + 'static>(
    chain_config: Arc<ChainConfig>,
    p2p_config: Arc<P2pConfig>,
    chainstate_handle: subsystem::Handle<Box<dyn chainstate_interface::ChainstateInterface>>,
    mempool_handle: MempoolHandle,
    time_getter: TimeGetter,
    peerdb_storage: S,
    bind_addresses: Vec<SocketAddr>,
    call: CallRequest<dyn P2pInterface>,
    shutdown: ShutdownRequest,
) -> Result<()> {
    if let Some(true) = p2p_config.disable_noise {
        assert_eq!(*chain_config.chain_type(), ChainType::Regtest);
        assert!(p2p_config.socks5_proxy.is_none());

        let transport = make_p2p_transport_unencrypted();

        P2p::<P2pNetworkingServiceUnencrypted>::new(
            transport,
            bind_addresses,
            chain_config,
            p2p_config,
            chainstate_handle,
            mempool_handle,
            time_getter,
            peerdb_storage,
        )
        .await?
        .run(call, shutdown)
        .await;
    } else if let Some(socks5_proxy) = &p2p_config.socks5_proxy {
        let transport = make_p2p_transport_socks5_proxy(socks5_proxy);

        P2p::<P2pNetworkingServiceSocks5Proxy>::new(
            transport,
            bind_addresses,
            chain_config,
            p2p_config,
            chainstate_handle,
            mempool_handle,
            time_getter,
            peerdb_storage,
        )
        .await?
        .run(call, shutdown)
        .await;
    } else {
        let transport = make_p2p_transport();

        P2p::<P2pNetworkingService>::new(
            transport,
            bind_addresses,
            chain_config,
            p2p_config,
            chainstate_handle,
            mempool_handle,
            time_getter,
            peerdb_storage,
        )
        .await?
        .run(call, shutdown)
        .await;
    }

    Ok(())
}
