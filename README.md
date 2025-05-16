# dag-network
### DAG Network, written in Rust

This project is a demo DAG Network. It is meant to be used as a base for further research into zero knowledge DAG systems. 

### To Run:
- compile lemutia-contracts:
  - `cd lemuria-contracts/`
  - `rustup target add wasm32-unknown-unknown`
  - `cargo build --release --target wasm32-unknown-unknown` 

- from project root, `cargo run` multiple node instances
- enter some key seed and ports for each instance
- run `call_api.sh`
  - this will be our nodes' controller
- enter api port of `node_0` (our bootstrap node)
- run `init-dag` command to initialize genesis blocks and initial contracts on bootstrap node
  - note the address for the `registry` contract in the logs, this contract is used to deploy other contracts
- for each other node:
  - run `port` command to switch to the api port of `node_i`
  - run `bootstrap` command, providing the bootstrap node's multiaddr
    - can be copied from the log `Now listening on /ip4/192.168.1.11/tcp/1110/p2p/12D3KooWL4u77srLWoVh5VWLrh3a9GfUgTGES58uHNYxcVCnF2h6`
    - example multiaddr: `/ip4/192.168.1.11/tcp/1110/p2p/12D3KooWL4u77srLWoVh5VWLrh3a9GfUgTGES58uHNYxcVCnF2h6`
  - run `sync-dag` to sync with bootstrap node
    - copy last portion of previous multiaddr for peer id 
    - example: `12D3KooWL4u77srLWoVh5VWLrh3a9GfUgTGES58uHNYxcVCnF2h6`

- once a node is bootstrapped and synced, you can run send-tx, get-tx, etc. (run help for more info)
- to send tx, provide contract address, function, and args
  - to upload contract, call `registry` contract (logged by boot node)

Project meant for demo purposes only.
