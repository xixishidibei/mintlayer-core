use crate::{
    chain::{
        signature::{
            inputsig::{InputWitness, StandardInputSignature},
            sighashtype::SigHashType,
            verify_signature, TransactionSigError,
        },
        Destination, Transaction, TransactionCreationError, TxInput, TxOutput,
    },
    primitives::{amount::IntType, Amount, Id, H256},
};
use crypto::key::PrivateKey;
use rand::Rng;


// This is required because we can't access private fields of the Transaction class
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutableTransaction {
    pub flags: u32,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub lock_time: u32,
}

impl TryFrom<&Transaction> for MutableTransaction {
    type Error = &'static str;

    fn try_from(tx: &Transaction) -> Result<Self, Self::Error> {
        Ok(Self {
            flags: tx.get_flags(),
            inputs: tx.get_inputs().clone(),
            outputs: tx.get_outputs().clone(),
            lock_time: tx.get_lock_time(),
        })
    }
}

impl MutableTransaction {
    pub fn generate_tx(&self) -> Result<Transaction, TransactionCreationError> {
        Transaction::new(
            self.flags,
            self.inputs.clone(),
            self.outputs.clone(),
            self.lock_time,
        )
    }
}

pub fn generate_unsigned_tx(
    outpoint_dest: Destination,
    inputs_count: u32,
    outputs_count: u32,
) -> Result<Transaction, TransactionCreationError> {
    let mut rng = rand::thread_rng();
    let mut inputs = Vec::new();
    for input_index in 0..inputs_count {
        inputs.push(TxInput::new(
            Id::<Transaction>::new(&H256::random()).into(),
            input_index,
            InputWitness::NoSignature(None),
        ));
    }
    let mut outputs = Vec::new();
    for _ in 0..outputs_count {
        outputs.push(TxOutput::new(
            Amount::from_atoms(rng.gen::<IntType>()),
            outpoint_dest.clone(),
        ));
    }

    let tx = Transaction::new(0, inputs, outputs, 0)?;
    Ok(tx)
}

pub fn sign_whole_tx(
    tx: &mut Transaction,
    private_key: &PrivateKey,
    sighash_type: SigHashType,
    outpoint_dest: Destination,
) -> Result<(), TransactionSigError> {
    for i in 0..tx.get_inputs().len() {
        update_signature(tx, i, private_key, sighash_type, outpoint_dest.clone())?;
    }
    Ok(())
}

pub fn update_signature(
    tx: &mut Transaction,
    input_num: usize,
    private_key: &PrivateKey,
    sighash_type: SigHashType,
    outpoint_dest: Destination,
) -> Result<(), TransactionSigError> {
    let input_sign = StandardInputSignature::produce_signature_for_input(
        private_key,
        sighash_type,
        outpoint_dest,
        tx,
        input_num,
    )?;
    tx.update_witness(input_num, InputWitness::Standard(input_sign)).unwrap();
    Ok(())
}

pub fn verify_signed_tx(
    tx: &Transaction,
    outpoint_dest: &Destination,
) -> Result<(), TransactionSigError> {
    for i in 0..tx.get_inputs().len() {
        verify_signature(outpoint_dest, tx, i)?
    }
    Ok(())
}
