use std::collections::HashMap;
use libp2p::PeerId;
use crate::dag::{StateNode, Transaction, TxHash, ZkProof};

#[derive(Debug, Clone)]
pub struct Zkvm {

}

impl Zkvm {

    pub fn new() -> Self {
        Self {}
    }

    pub fn execute_tx_circuit(
        &mut self,
        parent_1_tx: Transaction,
        parent_1_hash: TxHash,
        parent_2_tx: Transaction,
        parent_2_hash: TxHash,
        contract_hash: TxHash,
        contract_bytecode: Vec<u8>,
        contract_function: String,
        contract_args: HashMap<String, Vec<u8>>,
        // peer_id: String,
        // private_key: String,
    ) -> (TxHash, Transaction, Vec<StateNode>, ZkProof) {
        /*
        - args: parent_1_tx, parent_2_tx, contract_hash, contract_bytecode, contract_function, contract_args, peer_id, private_key
        - verify parent_1_tx and parent_2_tx
            - prove:
                - no conflicts exist in parents states
                - zk proofs are valid
                - leads to given state change
        - merge parent states
        - if conflict arises, return Err("Conflicting parents")
        - verify hash(private_key) == peer_id
        - hash contract bytecode and verify it matches hash (proof correct contract was run)
        - execute contract code, given functions + args
        - update state tree path, calculating new state root
        - construct Transaction object
        - hash Transaction object
        - returns: tx_hash, tx_object, state_tree_path, zk_proof
         */
        unimplemented!()
    }
}

