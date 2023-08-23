// Copyright (c) 2023 RBB S.r.l
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

use std::{net::SocketAddr, str::FromStr, sync::Arc};

use api_server_common::storage::impls::sqlx::SqlxStorage;
use blockchain_scanner_lib::blockchain_state::BlockchainState;
use clap::Parser;
use common::chain::{config::ChainType, ChainConfig};
use config::ApiServerScannerArgs;
use node_comm::{make_rpc_client, rpc_client::NodeRpcClient};
use rpc::RpcAuthData;
use utils::{cookie::COOKIE_FILENAME, default_data_dir::default_data_dir_for_chain};
mod config;

pub async fn run(
    chain_config: &Arc<ChainConfig>,
    rpc_client: &NodeRpcClient,
) -> Result<(), ApiServerScannerError> {
    // TODO: move this storage initialization into a separate function... the trait bounds are gonna be painful
    let mut storage = SqlxStorage::from_sqlite_inmemory(8)
        .await
        .unwrap_or_else(|e| panic!("Storage initialization failed {}", e))
        .into_transactional();

    {
        let mut db_tx = storage
            .transaction_rw()
            .await
            .unwrap_or_else(|e| panic!("Initial transaction for initialization failed {}", e));
        if !db_tx
            .is_initialized()
            .await
            .unwrap_or_else(|e| panic!("Storage initialization checking failed {}", e))
        {
            db_tx
                .initialize_database()
                .await
                .unwrap_or_else(|e| panic!("Storage initialization failed {}", e));
        }
    }

    let mut local_block = BlockchainState::new(storage);
    loop {
        let sync_result =
            blockchain_scanner_lib::sync::sync_once(chain_config, rpc_client, &mut local_block)
                .await;

        match sync_result {
            Ok(_) => (),
            Err(err) => logging::log::error!("Scanner sync error: {}", err),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ApiServerScannerError {
    #[error("RPC error: {0}")]
    RpcError(node_comm::rpc_client::NodeRpcError),
    #[error("Invalid config: {0}")]
    InvalidConfig(String),
}

#[tokio::main]
async fn main() -> Result<(), ApiServerScannerError> {
    utils::rust_backtrace::enable();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    let args = ApiServerScannerArgs::parse();

    logging::init_logging::<&std::path::Path>(None);
    logging::log::info!("Command line options: {args:?}");

    let ApiServerScannerArgs {
        network,
        rpc_address,
        rpc_cookie_file,
        rpc_username,
        rpc_password,
    } = args;

    let chain_type: ChainType = network.into();
    let chain_config = Arc::new(common::chain::config::Builder::new(chain_type).build());

    let rpc_auth = match (rpc_cookie_file, rpc_username, rpc_password) {
        (None, None, None) => {
            let cookie_file_path =
                default_data_dir_for_chain(chain_type.name()).join(COOKIE_FILENAME);
            RpcAuthData::Cookie { cookie_file_path }
        }
        (Some(cookie_file_path), None, None) => RpcAuthData::Cookie {
            cookie_file_path: cookie_file_path.into(),
        },
        (None, Some(username), Some(password)) => RpcAuthData::Basic { username, password },
        _ => {
            return Err(ApiServerScannerError::InvalidConfig(
                "Invalid RPC cookie/username/password combination".to_owned(),
            ))
        }
    };

    // TODO: Use the constant with the node
    let default_http_rpc_addr = || SocketAddr::from_str("127.0.0.1:3030").expect("Can't fail");
    let rpc_address = rpc_address.unwrap_or_else(default_http_rpc_addr);

    let rpc_client = make_rpc_client(rpc_address.to_string(), rpc_auth)
        .await
        .map_err(ApiServerScannerError::RpcError)?;

    {
        run(&chain_config, &rpc_client).await?;
    }
    Ok(())
}
