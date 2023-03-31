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

use crate::chain::{ChainConfig, SignedTransaction, TxInput};

use self::{
    inputsig::{
        classical_multisig::{
            authorize_classical_multisig::ClassicalMultisigSigningError,
            multisig_partial_signature::PartiallySignedMultisigStructureError,
        },
        standard_signature::StandardInputSignature,
        InputWitness,
    },
    sighash::signature_hash,
};

pub mod inputsig;
pub mod sighash;
pub mod sighashtype;

use thiserror::Error;

use super::{Destination, Transaction, TxOutput};

#[derive(Error, Debug, PartialEq, Eq, Clone)]
pub enum TransactionSigError {
    #[error("Invalid sighash value provided")]
    InvalidSigHashValue(u8),
    #[error("Invalid input index was provided (provided: `{0}` vs available: `{1}`")]
    InvalidInputIndex(usize, usize),
    #[error("Utxos count does not match inputs count (Utxo count: `{0}` vs inputs: `{1}`")]
    InvalidUtxoCountVsInputs(usize, usize),
    #[error("Invalid signature index was provided (provided: `{0}` vs available: `{1}`")]
    InvalidSignatureIndex(usize, usize),
    #[error("Requested signature hash without the presence of any inputs")]
    SigHashRequestWithoutInputs,
    #[error("Attempted to verify signatures for a transaction without inputs")]
    SignatureVerificationWithoutInputs,
    #[error("Attempted to verify signatures for a transaction without signatures")]
    SignatureVerificationWithoutSigs,
    #[error("Input corresponding to output number {0} does not exist (number of inputs is {1})")]
    InvalidOutputIndexForModeSingle(usize, usize),
    #[error("Decoding witness failed ")]
    DecodingWitnessFailed,
    #[error("Signature verification failed ")]
    SignatureVerificationFailed,
    #[error("Public key to address mismatch")]
    PublicKeyToAddressMismatch,
    #[error("Address authorization decoding failed")]
    AddressAuthDecodingFailed(String),
    #[error("Signature decoding failed")]
    InvalidSignatureEncoding,
    #[error("No signature!")]
    SignatureNotFound,
    #[error("Producing signature failed!")]
    ProducingSignatureFailed(crypto::key::SignatureError),
    #[error("Private key does not match with spender public key")]
    SpendeePrivatePublicKeyMismatch,
    #[error("AnyoneCanSpend should not use standard signatures, this place should be unreachable")]
    AttemptedToVerifyStandardSignatureForAnyoneCanSpend,
    #[error("AnyoneCanSpend should not use standard signatures, so producing a signature for it is not possible")]
    AttemptedToProduceSignatureForAnyoneCanSpend,
    #[error("Classical multisig signature attempted in uni-party function")]
    AttemptedToProduceClassicalMultisigSignatureForAnyoneCanSpend,
    #[error("Number of signatures does not match number of inputs")]
    InvalidWitnessCount,
    #[error("Invalid classical multisig challenge")]
    InvalidClassicalMultisig(#[from] PartiallySignedMultisigStructureError),
    #[error("Incomplete classical multisig signature(s)")]
    IncompleteClassicalMultisigSignature,
    #[error("Invalid classical multisig signature(s)")]
    InvalidClassicalMultisigSignature,
    #[error("The hash provided does not match the hash in the witness")]
    ClassicalMultisigWitnessHashMismatch,
    #[error("Producing classical multisig signing failed: {0}")]
    ClassicalMultisigSigningFailed(#[from] ClassicalMultisigSigningError),
    #[error("Standard signature creation failed. Invalid classical multisig authorization")]
    InvalidClassicalMultisigAuthorization,
    #[error("Standard signature creation failed. Incomplete classical multisig authorization")]
    IncompleteClassicalMultisigAuthorization,
    #[error("Unsupported yet!")]
    Unsupported,
}

pub trait Signable {
    fn inputs(&self) -> Option<&[TxInput]>;
    fn outputs(&self) -> Option<&[TxOutput]>;
    fn version_byte(&self) -> Option<u8>;
    fn lock_time(&self) -> Option<u32>;
    fn flags(&self) -> Option<u32>;
}

pub trait Transactable: Signable {
    fn signatures(&self) -> Option<&[InputWitness]>;
}

impl Signable for Transaction {
    fn inputs(&self) -> Option<&[TxInput]> {
        Some(self.inputs())
    }

    fn outputs(&self) -> Option<&[TxOutput]> {
        Some(self.outputs())
    }

    fn version_byte(&self) -> Option<u8> {
        Some(self.version_byte())
    }

    fn lock_time(&self) -> Option<u32> {
        Some(self.lock_time())
    }

    fn flags(&self) -> Option<u32> {
        Some(self.flags())
    }
}

impl Signable for SignedTransaction {
    fn inputs(&self) -> Option<&[TxInput]> {
        Some(self.inputs())
    }

    fn outputs(&self) -> Option<&[TxOutput]> {
        Some(self.outputs())
    }

    fn version_byte(&self) -> Option<u8> {
        Some(self.version_byte())
    }

    fn lock_time(&self) -> Option<u32> {
        Some(self.lock_time())
    }

    fn flags(&self) -> Option<u32> {
        Some(self.flags())
    }
}

impl Transactable for SignedTransaction {
    fn signatures(&self) -> Option<&[InputWitness]> {
        Some(self.signatures())
    }
}

pub fn verify_signature<T: Transactable>(
    chain_config: &ChainConfig,
    outpoint_destination: &Destination,
    tx: &T,
    inputs_utxos: &[&TxOutput],
    input_num: usize,
) -> Result<(), TransactionSigError> {
    let inputs = tx.inputs().ok_or(TransactionSigError::SignatureVerificationWithoutInputs)?;
    let sigs = tx.signatures().ok_or(TransactionSigError::SignatureVerificationWithoutSigs)?;
    let input_witness = sigs.get(input_num).ok_or(TransactionSigError::InvalidSignatureIndex(
        input_num,
        inputs.len(),
    ))?;

    match input_witness {
        InputWitness::NoSignature(_) => match outpoint_destination {
            Destination::Address(_)
            | Destination::PublicKey(_)
            | Destination::ScriptHash(_)
            | Destination::ClassicMultisig(_) => {
                return Err(TransactionSigError::SignatureNotFound)
            }
            Destination::AnyoneCanSpend => {}
        },
        InputWitness::Standard(witness) => verify_standard_input_signature(
            chain_config,
            outpoint_destination,
            witness,
            tx,
            inputs_utxos,
            input_num,
        )?,
    }
    Ok(())
}

fn verify_standard_input_signature<T: Transactable>(
    chain_config: &ChainConfig,
    outpoint_destination: &Destination,
    witness: &StandardInputSignature,
    tx: &T,
    inputs_utxos: &[&TxOutput],
    input_num: usize,
) -> Result<(), TransactionSigError> {
    let sighash = signature_hash(witness.sighash_type(), tx, inputs_utxos, input_num)?;
    witness.verify_signature(chain_config, outpoint_destination, &sighash)?;
    Ok(())
}

#[cfg(test)]
mod tests;
