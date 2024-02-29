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

use std::path::PathBuf;

use common::chain::config::{regtest_options::ChainConfigOptions, ChainType};
use crypto::key::hdkd::u31::U31;
use rpc::{
    rpc_creds::{RpcCreds, RpcCredsError},
    RpcAuthData,
};
use utils::{
    clap_utils, cookie::COOKIE_FILENAME, default_data_dir::default_data_dir_for_chain, ensure,
};
use utils_networking::NetworkAddressWithPort;

use crate::config::{WalletRpcConfig, WalletServiceConfig};

/// Service providing an RPC interface to a wallet
#[derive(clap::Parser)]
pub struct WalletRpcDaemonArgs {
    #[clap(subcommand)]
    command: WalletRpcDaemonCommand,
}

impl WalletRpcDaemonArgs {
    pub fn into_config(self) -> Result<(WalletServiceConfig, WalletRpcConfig), ConfigError> {
        let Self { command } = self;
        command.into_config()
    }
}

#[derive(clap::Subcommand)]
pub enum WalletRpcDaemonCommand {
    /// Run the mainnet wallet.
    Mainnet(WalletRpcDaemonChainArgs),

    /// Run the testnet wallet.
    Testnet(WalletRpcDaemonChainArgs),

    /// Run the regtest wallet.
    Regtest {
        #[command(flatten)]
        args: WalletRpcDaemonChainArgs,
        #[command(flatten)]
        regtest_opts: Box<ChainConfigOptions>,
    },
}

impl WalletRpcDaemonCommand {
    fn into_config(self) -> Result<(WalletServiceConfig, WalletRpcConfig), ConfigError> {
        match self {
            Self::Mainnet(args) => args.into_config(ChainType::Mainnet),
            Self::Testnet(args) => args.into_config(ChainType::Testnet),
            Self::Regtest { args, regtest_opts } => {
                let (ws_config, rpc_config) = args.into_config(ChainType::Regtest)?;
                let ws_config = ws_config
                    .with_regtest_options(*regtest_opts)
                    .map_err(ConfigError::InvalidRegtestOptions)?;
                Ok((ws_config, rpc_config))
            }
        }
    }
}

#[derive(clap::Args)]
#[clap(mut_args(clap_utils::env_adder("WALLET_RPC_DAEMON")))]
#[command(
    version,
    about,
    group(
        clap::ArgGroup::new("rpc_auth")
            .args(["rpc_cookie_file", "rpc_username", "rpc_password", "rpc_no_authentication"])
            .required(true)
            .multiple(true)
    ),
)]
pub struct WalletRpcDaemonChainArgs {
    /// The wallet file to operate on
    #[arg()]
    wallet_path: Option<PathBuf>,

    /// Start staking for the specified account after starting the wallet
    #[arg(long, value_name("ACC_NUMBER"), requires("wallet_path"))]
    start_staking_for_account: Vec<U31>,

    /// RPC address of the node to connect to
    #[arg(long, value_name("ADDR"))]
    node_rpc_address: Option<NetworkAddressWithPort>,

    /// Node RPC authentication cookie file path
    #[arg(
        long,
        value_name("PATH"),
        conflicts_with_all(["node_username", "node_password"])
    )]
    node_cookie_file: Option<PathBuf>,

    /// Node login user name
    #[arg(long, value_name("NAME"), requires("node_password"))]
    node_username: Option<String>,

    /// Node login password
    #[arg(long, value_name("PASS"), requires("node_username"))]
    node_password: Option<String>,

    /// Address to bind the wallet RPC interface to
    #[arg(long, value_name("ADDR"))]
    rpc_bind_address: Option<String>,

    /// Custom file path for the RPC cookie file.
    /// If not set, the cookie file is created in the data dir.
    #[arg(long, value_name("PATH"), conflicts_with_all(["rpc_username", "rpc_password"]))]
    rpc_cookie_file: Option<PathBuf>,

    /// Username for RPC server basic authorization.
    /// If not set, the cookie file is created.
    #[arg(long, value_name("USER"), requires("rpc_password"))]
    rpc_username: Option<String>,

    /// Password for RPC server basic authorization.
    /// If not set, the RPC cookie file is created.
    #[arg(long, value_name("PASS"), requires("rpc_username"))]
    rpc_password: Option<String>,

    /// Enable running the wallet service without RPC authentication
    #[arg(long, conflicts_with_all(["rpc_password", "rpc_username", "rpc_cookie_file"]))]
    rpc_no_authentication: bool,
}

impl WalletRpcDaemonChainArgs {
    fn into_config(
        self,
        chain_type: ChainType,
    ) -> Result<(WalletServiceConfig, WalletRpcConfig), ConfigError> {
        let Self {
            wallet_path: wallet_file,
            rpc_bind_address: wallet_rpc_address,
            start_staking_for_account,
            node_rpc_address,
            node_cookie_file,
            node_username,
            node_password,
            rpc_cookie_file,
            rpc_username,
            rpc_password,
            rpc_no_authentication,
        } = self;

        let ws_config = {
            // Node RPC authentication
            let node_credentials = match (node_cookie_file, node_username, node_password) {
                (Some(cookie_file_path), None, None) => RpcAuthData::Cookie { cookie_file_path },
                (None, Some(username), Some(password)) => RpcAuthData::Basic { username, password },
                (None, None, None) => {
                    let cookie_file_path =
                        default_data_dir_for_chain(chain_type.name()).join(COOKIE_FILENAME);
                    RpcAuthData::Cookie { cookie_file_path }
                }
                _ => panic!("Should not happen due to arg constraints"),
            };

            WalletServiceConfig::new(chain_type, wallet_file, start_staking_for_account)
                .apply_option(
                    WalletServiceConfig::with_node_rpc_address,
                    node_rpc_address.map(|addr| addr.to_string()),
                )
                .with_node_credentials(node_credentials)
        };

        let rpc_config = make_wallet_config(
            rpc_cookie_file,
            rpc_username,
            rpc_password,
            rpc_no_authentication,
            wallet_rpc_address,
            *ws_config.chain_config.chain_type(),
        )?;

        Ok((ws_config, rpc_config))
    }
}

pub fn make_wallet_config(
    rpc_cookie_file: Option<PathBuf>,
    rpc_username: Option<String>,
    rpc_password: Option<String>,
    rpc_no_authentication: bool,
    wallet_rpc_address: Option<String>,
    chain_type: ChainType,
) -> Result<WalletRpcConfig, ConfigError> {
    let rpc_config = {
        // Credentials used to access the wallet RPC interface
        let auth_credentials = match (rpc_cookie_file, rpc_username, rpc_password) {
            (Some(cookie_file), None, None) => Some(RpcCreds::cookie_file(cookie_file)?),
            (None, Some(user), Some(pass)) => Some(RpcCreds::basic(user, pass)?),
            (None, None, None) => {
                ensure!(rpc_no_authentication, ConfigError::NoAuth);
                None
            }
            _ => panic!("Should not happen due to arg constraints"),
        };

        let bind_addr = match wallet_rpc_address {
            None => {
                let port = WalletRpcConfig::default_port(chain_type);
                std::net::SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), port)
            }
            Some(addr) => addr.parse().map_err(ConfigError::InvalidRpcBindAddr)?,
        };

        WalletRpcConfig {
            bind_addr,
            auth_credentials,
        }
    };
    Ok(rpc_config)
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error(transparent)]
    RpcCreds(#[from] RpcCredsError),

    #[error("Please specify authentication method")]
    NoAuth,

    #[error("Invalid wallet RPC bind address: {0}")]
    InvalidRpcBindAddr(std::net::AddrParseError),

    #[error("Invalid chain config option: {0}")]
    InvalidChainConfigOption(String),

    #[error("Invalid regtest chain configuration: {0}")]
    InvalidRegtestOptions(anyhow::Error),
}
