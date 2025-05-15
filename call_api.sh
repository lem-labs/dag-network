#!/usr/bin/env bash

print_help() {
  echo ""
  echo "Available commands:"
  printf "%-20s %s\n" \
    "bootstrap"         "Bootstrap to a peer via multiaddr" \
    "send-tx"           "Send a transaction with contract/function/args" \
    "sync-dag"          "Sync DAG from peer list" \
    "get-sync-status"   "Get DAG sync status" \
    "get-tx"            "Fetch transaction by ID" \
    "get-txs"           "Fetch multiple transactions by IDs" \
    "get-ancestry"      "Get parent history of a transaction" \
    "get-peers"         "List connected peers" \
    "get-known-addrs"   "List known addresses" \
    "self-peer-id"      "Get your peer ID" \
    "self-addresses"    "Get your listening addresses" \
    "dial-peer"         "Dial a peer by peer ID" \
    "dag-height"        "Get DAG height" \
    "metrics"           "Fetch metrics" \
    "shutdown"          "Shut down the node" \
    "port"              "Change port (default: 3030)" \
    "help"              "Show this help message" \
    "quit"              "Exit the CLI"
  echo ""
}


read -p "Enter API port (default 3030): " port
  port=${port:-3030}

echo ""
echo "Available commands:"
printf "%-20s %-20s %-20s\n" \
  "bootstrap"         "init-dag"          "sync-dag" \
  "send-tx"           "get-tx"            "get-txs" \
  "get-ancestry"      "get-peers"         "get-known-addrs" \
  "self-peer-id"      "self-addresses"    "dial-peer" \
  "metrics"           "dag-height"        "shutdown" \
  "port"              "help"              "quit" \

echo ""

while true; do
  read -p "> " cmd

  case $cmd in
    port)
      read -p "Enter new port: " new_port
      port=${new_port:-3030}
      echo "Port set to $port"
      ;;
    bootstrap)
      read -p "Bootstrap address: " addr
      curl -X POST http://localhost:$port/bootstrap -H "Content-Type: application/json" -d "{\"multiaddr\": \"$addr\"}"
      ;;
    init-dag)
          curl -X GET http://localhost:$port/initDag;;
    send-tx)
      read -p "Contract address: " contract
      read -p "Function name: " function
      read -p "Number of args: " n
      args="{"
      for ((i=0; i<n; i++)); do
        read -p "Arg $i key: " key
        read -p "Arg $i (hex string): " val
        bytes=$(echo "$val" | xxd -r -p | od -An -t u1 | tr -s ' ' | sed 's/^ *//' | tr ' ' ',')
        args+="\"$key\":[$bytes]"
        if (( i < n - 1 )); then args+=","; fi
      done
      args+="}"
      curl -X POST http://localhost:$port/sendTx -H "Content-Type: application/json" -d "{\"contract\": \"$contract\", \"function\": \"$function\", \"args\": $args}"
      ;;
    sync-dag)
      read -p "Peer ID: " peer
      curl -X POST http://localhost:$port/syncDag -H "Content-Type: application/json" -d "{\"peer\": \"$peer\"}"
      ;;
    get-tx)
      read -p "Tx ID: " tx
      curl -X POST http://localhost:$port/getTx -H "Content-Type: application/json" -d "{\"tx_id\": \"$tx\"}"
      ;;
    get-txs)
      read -p "Comma-separated Tx IDs: " ids
      id_json=$(echo "$ids" | sed 's/[^,]*/"&"/g')
      curl -X POST http://localhost:$port/getTxs -H "Content-Type: application/json" -d "{\"tx_ids\": [ $id_json ]}"
      ;;
    get-ancestry)
      read -p "Tx ID: " tx
      read -p "Levels: " levels
      curl -X POST http://localhost:$port/getAncestry -H "Content-Type: application/json" -d "{\"tx_id\": \"$tx\", \"levels\": $levels}"
      ;;
    get-peers)
      curl http://localhost:$port/getPeers ;;
    get-known-addrs)
      curl http://localhost:$port/getKnownAddresses ;;
    self-peer-id)
      curl http://localhost:$port/selfPeerId ;;
    self-addresses)
      curl http://localhost:$port/selfAddress ;;
    dial-peer)
      read -p "Peer ID: " pid
      curl -X POST http://localhost:$port/dialPeer -H "Content-Type: application/json" -d "{\"peer_id\": \"$pid\"}"
      ;;
    dag-height)
      curl http://localhost:$port/dagHeight ;;
    metrics)
      curl http://localhost:$port/metrics ;;
    shutdown)
      curl -X POST http://localhost:$port/shutdown ;;
    help)
      print_help ;;
    quit|q|exit)
      echo "Goodbye!"
      break ;;
    *)
      echo "Unknown command. Type 'help' for a list."
      ;;
  esac

  echo ""
done
