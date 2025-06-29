# Fooswap Backend

A Rust-based off-chain indexer and HTTP API for the Fooswap decentralised exchange (DEX) on the Sui Network.

## Overview

This project provides a backend for indexing and serving DEX data from Sui blockchain events. It includes:

- **Blockchain Indexer**: Monitors Sui Move events for pool creation and swap transactions
- **REST API**: HTTP endpoints for querying pool data, swap history, and price calculations
- **SQLite Database**: Local storage for efficient data persistence and querying

## Features

- **Real-time Event Indexing**: Polls Sui RPC for `PoolCreatedEvent` and `SwapEvent` events
- **Automatic Data Persistence**: Stores pool and swap data in SQLite with proper indexing
- **RESTful API**: HTTP endpoints for DEX data access
- **Price Calculation**: Computes prices using the constant product formula
- **Transaction Deduplication**: Avoids duplicate processing using transaction digests
- **Health Check**: Simple endpoint to check if the service is running

## Quick Start

### Prerequisites

- Rust 1.70+ and Cargo
- Sui CLI (for contract deployment)
- Access to a Sui RPC endpoint

### Installation

1. Clone the repository:
```bash
git clone <repository-url>
cd fooswap-backend
```

2. Build the project:
```bash
cargo build
```

3. Set your environment variables:
```bash
# Sui RPC endpoint (defaults to devnet)
export SUI_RPC_URL=https://fullnode.devnet.sui.io:443
```

4. Run the application:
```bash
cargo run
```

The server will start on `http://127.0.0.1:3000`

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SUI_RPC_URL` | `https://fullnode.devnet.sui.io:443` | Sui RPC endpoint URL |
| `DEX_PACKAGE_ID` | Hardcoded in `src/indexer.rs` | Sui Move package ID for the DEX contract |

### Updating Package ID

If you deploy to a different network or update your contract, change the `DEX_PACKAGE_ID` constant in `src/indexer.rs`:

```rust
const DEX_PACKAGE_ID: &str = "0xYOUR_NEW_PACKAGE_ID";
```

## API Reference

### Health Check
```http
GET /health
```
Returns `OK` if the service is running.

### List All Pools
```http
GET /api/pools
```

**Response:**
```json
{
  "status": "ok",
  "data": [
    {
      "pool_id": "0x...",
      "token_a": "0x...",
      "token_b": "0x...",
      "reserve_a": 1000.0,
      "reserve_b": 500.0,
      "last_updated": 1751104133893
    }
  ]
}
```

### Get Pool Swap History
```http
GET /api/swaps/{pool_id}
```

**Parameters:**
- `pool_id`: The unique identifier of the liquidity pool

**Response:**
```json
{
  "status": "ok",
  "data": [
    {
      "pool_id": "0x...",
      "amount_in": 100.0,
      "amount_out": 50.0,
      "timestamp": 1751104259632
    }
  ]
}
```

### Calculate Token Price
```http
GET /api/price?pair=TOKENA/TOKENB
```

**Parameters:**
- `pair`: Token pair in the format "TOKENA/TOKENB" (e.g. "USDC/SUI")

**Response:**
```json
{
  "status": "ok",
  "pair": "USDC/SUI",
  "pool_id": "0x...",
  "price": 0.5
}
```

## Database Schema

### Pools Table
Stores the current state of all liquidity pools:

```sql
CREATE TABLE pools (
    pool_id     TEXT PRIMARY KEY,
    token_a     TEXT NOT NULL,
    token_b     TEXT NOT NULL,
    reserve_a   REAL NOT NULL DEFAULT 0.0,
    reserve_b   REAL NOT NULL DEFAULT 0.0,
    last_updated INTEGER NOT NULL DEFAULT 0
);
```

### Swaps Table
Stores historical swap transactions:

```sql
CREATE TABLE swaps (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    pool_id      TEXT NOT NULL,
    amount_in    REAL NOT NULL,
    amount_out   REAL NOT NULL,
    timestamp    INTEGER NOT NULL,
    tx_digest    TEXT NOT NULL UNIQUE
);
```

## Architecture

### Core Components

- **`src/main.rs`**: Application entry point and server setup
- **`src/indexer.rs`**: Blockchain event polling, parsing, and database persistence
- **`src/routes.rs`**: HTTP API endpoint handlers and response formatting
- **`src/db.rs`**: Database operations and schema management

### Data Flow

1. The indexer polls Sui RPC every 5 seconds for new events
2. Event processing extracts relevant data from Move events
3. The database stores pool and swap data with proper indexing
4. The API server serves HTTP requests with real-time data from SQLite

## Development

### Building
```bash
# Debug build
cargo build

# Release build
cargo build --release
```

### Testing
```bash
# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run
```

### Database Inspection
```bash
# Open the SQLite database
sqlite3 fooswap.db

# View tables
.tables

# Query data
SELECT * FROM pools;
SELECT * FROM swaps LIMIT 10;
```

## Docker

You can use Docker to build and run the backend:

```dockerfile
FROM rust:1.70 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/fooswap-backend /usr/local/bin/
EXPOSE 3000
CMD ["fooswap-backend"]
```
