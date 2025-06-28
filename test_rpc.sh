#!/bin/bash

curl -X POST https://fullnode.devnet.sui.io:443 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "suix_queryEvents",
    "params": [
      { "MoveEventType": "0x1c2be4cfbf91fe8d71aedeb83cbe680475b70359bab87900df99ecd787ca5474::fooswap::PoolCreatedEvent" },
      null,
      100,
      false
    ]
  }'