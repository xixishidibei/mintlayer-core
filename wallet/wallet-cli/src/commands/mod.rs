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

use clap::Parser;
use node_comm::node_traits::NodeInterface;
use reedline::Reedline;
use serialization::hex::HexEncode;
use wallet::DefaultWallet;

use crate::{cli_println, errors::WalletCliError, output::OutputContext};

#[derive(Debug, Parser)]
#[clap(rename_all = "lower")]
pub enum WalletCommands {
    /// Returns the current best block hash
    BestBlock,

    /// Returns the current best block height
    BestBlockHeight,

    /// Submit a block to be included in the chain
    SubmitBlock {
        /// Hex encoded block
        block: String,
    },

    /// Rescan
    Rescan,

    /// Quit the REPL
    Exit,

    /// Print history
    History,

    /// Clear screen
    #[clap(name = "clear")]
    ClearScreen,

    /// Clear history
    ClearHistory,
}

pub async fn handle_wallet_command(
    output: &OutputContext,
    rpc_client: &mut impl NodeInterface,
    _wallet: &mut DefaultWallet,
    line_editor: &mut Reedline,
    command: WalletCommands,
) -> Result<(), WalletCliError> {
    match command {
        WalletCommands::BestBlock => {
            let id = rpc_client
                .get_best_block_id()
                .await
                .map_err(|e| WalletCliError::RpcError(e.to_string()))?;
            cli_println!(output, "{}", id.hex_encode());
            Ok(())
        }

        WalletCommands::BestBlockHeight => {
            let height = rpc_client
                .get_best_block_height()
                .await
                .map_err(|e| WalletCliError::RpcError(e.to_string()))?;
            cli_println!(output, "{}", height);
            Ok(())
        }

        WalletCommands::SubmitBlock { block } => {
            rpc_client
                .submit_block(block)
                .await
                .map_err(|e| WalletCliError::RpcError(e.to_string()))?;
            cli_println!(output, "The block was submitted successfully");
            Ok(())
        }

        WalletCommands::Rescan => {
            cli_println!(output, "Not implemented");
            Ok(())
        }

        WalletCommands::Exit => Err(WalletCliError::Exit),
        WalletCommands::History => {
            line_editor.print_history().expect("Should not fail normally");
            Ok(())
        }
        WalletCommands::ClearScreen => {
            line_editor.clear_scrollback().expect("Should not fail normally");
            Ok(())
        }
        WalletCommands::ClearHistory => {
            line_editor.history_mut().clear().expect("Should not fail normally");
            Ok(())
        }
    }
}
