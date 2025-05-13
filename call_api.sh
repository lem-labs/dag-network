#!/usr/bin/env bash

read -p "Enter API port (default 3030): " port
port=${port:-3030}

echo "Available commands:"
echo "1) send_tx"
echo "2) tx_status"
echo "3) dial_peer"
echo "4) get_peers"
echo "5) get_known_addresses"
echo "6) self_peer_id"
echo "7) bootstrap"
echo "8) get_account"
echo "9) get_contract"
echo "10) get_tx"
echo "11) get_parents"
echo "12) get_children"
echo "13) get_sync_status"
echo "14) dag_height"
echo "15) shutdown"
echo "16) metrics"

read -p "Choose a command number: " cmd

case $cmd in
  1)
    read -p "Contract (as string): " contract
    read -p "Function name: " function
    read -p "Number of args: " n
    args="[]"
    if [[ $n -gt 0 ]]; then
      args="["
      for ((i=0; i<n; i++)); do
        read -p "Arg $i (hex string, no 0x prefix): " arg
        args+="$([[ $i -gt 0 ]] && echo "," || echo "")\"$(echo $arg | xxd -r -p | base64)\""
      done
      args+="]"
    fi
    curl -X POST http://localhost:$port/send_tx -H "Content-Type: application/json" -d '{"contract": "'"$contract"'", "function": "'"$function"'", "args": '"$args"'}'
    ;;
  2)
    read -p "TxHash: " tx
    curl -X POST http://localhost:$port/tx_status -H "Content-Type: application/json" -d '{"tx_id": "'"$tx"'"}'
    ;;
  3)
    read -p "Peer ID: " pid
    curl -X POST http://localhost:$port/dial_peer -H "Content-Type: application/json" -d '{"peer_id": "'"$pid"'"}'
    ;;
  4)
    curl http://localhost:$port/get_peers ;;
  5)
    curl http://localhost:$port/get_known_addresses ;;
  6)
    curl http://localhost:$port/self_peer_id ;;
  7)
    read -p "Bootstrap multiaddr (empty to self-bootstrap): " addr
    curl -X POST http://localhost:$port/bootstrap -H "Content-Type: application/json" -d '{"multiaddr": "'"$addr"'"}'
    ;;
  8)
    read -p "Peer ID: " pid
    curl -X POST http://localhost:$port/get_account -H "Content-Type: application/json" -d '{"peer_id": "'"$pid"'"}'
    ;;
  9)
    read -p "Contract address: " addr
    curl -X POST http://localhost:$port/get_contract -H "Content-Type: application/json" -d '{"tx_id": "'"$addr"'"}'
    ;;
  10)
    read -p "Tx ID: " tx
    curl -X POST http://localhost:$port/get_tx -H "Content-Type: application/json" -d '{"tx_id": "'"$tx"'"}'
    ;;
  11)
    read -p "Tx ID: " tx
    curl -X POST http://localhost:$port/get_parents -H "Content-Type: application/json" -d '{"tx_id": "'"$tx"'"}'
    ;;
  12)
    read -p "Tx ID: " tx
    curl -X POST http://localhost:$port/get_children -H "Content-Type: application/json" -d '{"tx_id": "'"$tx"'"}'
    ;;
  13)
    curl http://localhost:$port/get_sync_status ;;
  14)
    curl http://localhost:$port/dag_height ;;
  15)
    curl -X POST http://localhost:$port/shutdown ;;
  16)
    curl http://localhost:$port/metrics ;;
  *)
    echo "Invalid selection"
    ;;
esac
