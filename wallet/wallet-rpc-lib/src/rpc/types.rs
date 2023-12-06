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

//! Types supporting the RPC interface

use std::collections::BTreeMap;

use common::{
    address::Address,
    chain::{tokens::TokenId, Destination, GenBlock, UtxoOutPoint},
    primitives::{Amount, BlockHeight, Id},
};
use crypto::key::hdkd::{child_number::ChildNumber, u31::U31};

pub use mempool_types::tx_options::TxOptionsOverrides;
pub use serialization::hex_encoded::HexEncoded;
use wallet::account::Currency;

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("Account index out of supported range")]
    AcctIndexOutOfRange,

    #[error("Invalid coin amount")]
    InvalidCoinAmount,

    #[error("Invalid address")]
    InvalidAddress,
}

impl From<RpcError> for rpc::Error {
    fn from(e: RpcError) -> Self {
        Self::owned::<()>(-1, e.to_string(), None)
    }
}

/// Struct representing empty arguments to RPC call, for forwards compatibility
#[derive(Debug, Eq, PartialEq, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EmptyArgs {}

#[derive(Debug, Eq, PartialEq, Clone, serde::Serialize, serde::Deserialize)]
pub struct AccountIndexArg {
    pub account: u32,
}

impl AccountIndexArg {
    pub fn index(&self) -> rpc::RpcResult<U31> {
        rpc::handle_result(U31::from_u32(self.account).ok_or(RpcError::AcctIndexOutOfRange))
    }
}

#[derive(Debug, Eq, PartialEq, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlockInfo {
    pub id: Id<GenBlock>,
    pub height: BlockHeight,
}

impl BlockInfo {
    pub fn from_tuple((id, height): (Id<GenBlock>, BlockHeight)) -> Self {
        Self { id, height }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, serde::Serialize, serde::Deserialize)]
pub struct AddressInfo {
    pub address: String,
    pub index: String,
}

impl AddressInfo {
    pub fn new(child_number: ChildNumber, address: Address<Destination>) -> Self {
        Self {
            address: address.to_string(),
            index: child_number.to_string(),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, serde::Serialize, serde::Deserialize)]
pub struct AddressWithUsageInfo {
    pub address: String,
    pub index: String,
    pub used: bool,
}

impl AddressWithUsageInfo {
    pub fn new(child_number: ChildNumber, address: Address<Destination>, used: bool) -> Self {
        Self {
            address: address.to_string(),
            index: child_number.to_string(),
            used,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, serde::Serialize, serde::Deserialize)]
pub struct BalanceInfo {
    pub coins: Amount,
    pub tokens: BTreeMap<TokenId, Amount>,
}

impl BalanceInfo {
    pub fn from_map(mut amounts: BTreeMap<Currency, Amount>) -> Self {
        let coins = amounts.remove(&Currency::Coin).unwrap_or(Amount::ZERO);
        let token_entry = |(curr, amt)| match curr {
            Currency::Coin => panic!("Coins removed in the previous step"),
            Currency::Token(tok_id) => (tok_id, amt),
        };
        let tokens = amounts.into_iter().map(token_entry).collect();
        Self { coins, tokens }
    }
}

// TODO(PR) use outputs directly
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UtxoInfo {
    pub outpoint: HexEncoded<UtxoOutPoint>,
    pub amount: Amount,
    pub currency: Currency,
    //pub timelock: OutputTimeLock, // TODO
    // TODO extend with more info
}
