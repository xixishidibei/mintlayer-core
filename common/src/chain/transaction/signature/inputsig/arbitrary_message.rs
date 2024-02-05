// Copyright (c) 2021-2024 RBB S.r.l
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

use thiserror::Error;

use serialization::Encode;

use crate::{
    chain::{signature::DestinationSigError, ChainConfig, Destination},
    primitives::{id::default_hash, H256},
};

use super::{
    authorize_pubkey_spend::{
        sign_pubkey_spending, verify_public_key_spending, AuthorizedPublicKeySpend,
    },
    authorize_pubkeyhash_spend::{
        sign_address_spending, verify_address_spending, AuthorizedPublicKeyHashSpend,
    },
    classical_multisig::authorize_classical_multisig::{
        verify_classical_multisig_spending, AuthorizedClassicalMultisigSpend,
    },
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SignArbitraryMessageError {
    #[error("Destination signature error: {0}")]
    DestinationSigError(#[from] DestinationSigError),
    #[error("AnyoneCanSpend should not use standard signatures, so producing a signature for it is not possible")]
    AttemptedToProduceSignatureForAnyoneCanSpend,
    #[error("Classical multisig signature attempted in uni-party function")]
    AttemptedToProduceClassicalMultisigSignatureInUnipartySignatureCode,
    #[error("Unsupported yet!")]
    Unsupported,
}

#[derive(Debug)]
pub struct SignedArbitraryMessage {
    raw_signature: Vec<u8>,
}

pub fn produce_message_challenge(message: &[u8]) -> H256 {
    let hashed_message = default_hash(&message);

    // Concatenate the message with its hash to prevent abusing signatures to trick users into signing transactions
    let message = message
        .iter()
        .chain(hashed_message.as_bytes().iter())
        .copied()
        .collect::<Vec<_>>();

    // Harden it further by hashing that twice. Now it's impossible to make this useful for a transaction
    default_hash(default_hash(message))
}

impl SignedArbitraryMessage {
    pub fn from_data(raw_signature: Vec<u8>) -> Self {
        Self { raw_signature }
    }

    pub fn to_hex(self) -> String {
        hex::encode(self.raw_signature)
    }

    pub fn verify_signature(
        &self,
        chain_config: &ChainConfig,
        outpoint_destination: &Destination,
        challenge: &H256,
    ) -> Result<(), DestinationSigError> {
        match outpoint_destination {
            Destination::PublicKeyHash(addr) => {
                let sig_components = AuthorizedPublicKeyHashSpend::from_data(&self.raw_signature)?;
                verify_address_spending(addr, &sig_components, challenge)?
            }
            Destination::PublicKey(pubkey) => {
                let sig_components = AuthorizedPublicKeySpend::from_data(&self.raw_signature)?;
                verify_public_key_spending(pubkey, &sig_components, challenge)?
            }
            Destination::ScriptHash(_) => return Err(DestinationSigError::Unsupported),
            Destination::AnyoneCanSpend => {
                // AnyoneCanSpend makes no sense for signing and verification.
                return Err(
                    DestinationSigError::AttemptedToVerifyStandardSignatureForAnyoneCanSpend,
                );
            }
            Destination::ClassicMultisig(h) => {
                let sig_components =
                    AuthorizedClassicalMultisigSpend::from_data(&self.raw_signature)?;
                verify_classical_multisig_spending(chain_config, h, &sig_components, challenge)?
            }
        }
        Ok(())
    }

    pub fn produce_uniparty_signature(
        private_key: &crypto::key::PrivateKey,
        outpoint_destination: &Destination,
        message: &[u8],
    ) -> Result<Self, SignArbitraryMessageError> {
        let challenge = produce_message_challenge(message);
        let signature =
            match outpoint_destination {
                Destination::PublicKeyHash(addr) => {
                    let sig = sign_address_spending(private_key, addr, &challenge)?;
                    sig.encode()
                }
                Destination::PublicKey(pubkey) => {
                    let sig = sign_pubkey_spending(private_key, pubkey, &challenge)?;
                    sig.encode()
                }
                Destination::ScriptHash(_) => return Err(SignArbitraryMessageError::Unsupported),

                Destination::AnyoneCanSpend => {
                    // AnyoneCanSpend makes no sense for signing and verification.
                    return Err(SignArbitraryMessageError::AttemptedToProduceSignatureForAnyoneCanSpend);
                }
                Destination::ClassicMultisig(_) => return Err(
                    // This function doesn't support this kind of signature
                    SignArbitraryMessageError::AttemptedToProduceClassicalMultisigSignatureInUnipartySignatureCode,
                ),
            };
        Ok(Self {
            raw_signature: signature,
        })
    }
}

#[cfg(test)]
#[path = "arbitrary_message_tests.rs"]
mod tests;
