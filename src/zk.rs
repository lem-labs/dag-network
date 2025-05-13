
#[derive(Debug, Clone)]
pub struct Zkvm {

}

impl Zkvm {

    pub fn new() -> Self {
        Self {}
    }

    pub fn execute_tx_circuit() {
        /*
        - args: parent_1_tx, parent_2_tx, contract_hash, contract_bytecode, contract_function, contract_args, peer_id, private_key
        - verify parent_1_tx and parent_2_tx
            - prove:
                - no conflicts exist in parents stattes
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

