// Copyright (c) 2022 RBB S.r.l
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

use super::{Block, Destination, Transaction};
use crate::primitives::{Amount, Id};
use serialization::{Decode, Encode};

mod nft;
mod rpc;
mod token_id;
mod tokens_utils;

pub use nft::*;
pub use rpc::*;
pub use token_id::TokenId;
pub use tokens_utils::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub struct TokenIssuanceVersion(u32);

impl TokenIssuanceVersion {
    /// Initial issuance implementation
    pub const V0: Self = Self(0);
    /// Add reissuance support
    pub const V1: Self = Self(1);

    pub const CURRENT: Self = Self::V1;
}

/// The data that is created when a token is issued to track it (and to update it with ACL commands)
#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq)]
pub struct TokenAuxiliaryData {
    issuance_tx: Transaction,
    issuance_block_id: Id<Block>,
}

impl TokenAuxiliaryData {
    pub fn new(issuance_tx: Transaction, issuance_block_id: Id<Block>) -> Self {
        Self {
            issuance_tx,
            issuance_block_id,
        }
    }

    pub fn issuance_tx(&self) -> &Transaction {
        &self.issuance_tx
    }

    pub fn issuance_block_id(&self) -> Id<Block> {
        self.issuance_block_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, serde::Serialize)]
pub struct TokenTransfer {
    pub token_id: TokenId,
    pub amount: Amount,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, serde::Serialize)]
pub struct TokenIssuance {
    pub token_ticker: Vec<u8>,
    pub amount_to_issue: Amount,
    pub number_of_decimals: u8,
    pub metadata_uri: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, serde::Serialize)]
pub enum TokenTotalSupply {
    Fixed(Amount), // fixed to a certain amount
    Lockable,      // not known in advance but can be locked at some point in time
    Unlimited,     // is limited only by the Amount data type
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, serde::Serialize)]
pub struct TokenIssuanceV1 {
    pub token_ticker: Vec<u8>,
    pub number_of_decimals: u8,
    pub metadata_uri: Vec<u8>,
    pub supply_limit: TokenTotalSupply,
    pub reissuance_controller: Destination,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize)]
pub enum TokenData {
    /// TokenTransfer data to another user. If it is a token, then the token data must also be transferred to the recipient.
    #[codec(index = 1)]
    TokenTransfer(TokenTransfer),
    /// New token creation
    #[codec(index = 2)]
    TokenIssuance(Box<TokenIssuance>),
    // A new NFT creation
    #[codec(index = 3)]
    NftIssuance(Box<NftIssuance>),
    /// New token creation with supply support
    #[codec(index = 4)]
    TokenIssuanceV1(Box<TokenIssuanceV1>),
}

impl From<NftIssuance> for TokenData {
    fn from(d: NftIssuance) -> Self {
        Self::NftIssuance(Box::new(d))
    }
}

impl From<TokenIssuance> for TokenData {
    fn from(d: TokenIssuance) -> Self {
        Self::TokenIssuance(Box::new(d))
    }
}
