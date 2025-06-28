use rusqlite::{params, Connection, Result};
use std::path::Path;

/// Initializes the SQLite database and creates the required schema.
/// 
/// This function creates the database file if it doesn't exist and sets up
/// the necessary tables for storing DEX pool and swap data. The database
/// is created in the project root directory as `fooswap.db`.
/// 
/// # Returns
/// * `Result<Connection>` - SQLite connection or error
/// 
/// # Database Schema
/// 
/// ## pools table
/// Stores current state of all liquidity pools:
/// - `pool_id`: Unique identifier for the pool (PRIMARY KEY)
/// - `token_a`: Address of the first token in the pair
/// - `token_b`: Address of the second token in the pair
/// - `reserve_a`: Current reserve of token A
/// - `reserve_b`: Current reserve of token B
/// - `last_updated`: Timestamp of last update
/// 
/// ## swaps table
/// Stores historical swap transactions:
/// - `id`: Auto-incrementing primary key
/// - `pool_id`: Reference to the pool where swap occurred
/// - `amount_in`: Amount of input token
/// - `amount_out`: Amount of output token
/// - `timestamp`: Transaction timestamp
/// - `tx_digest`: Unique transaction digest (UNIQUE constraint for deduplication)
pub fn init_db() -> Result<Connection> {
    // Database file path in project root
    let db_path = Path::new("fooswap.db");
    let conn = Connection::open(db_path)?;

    // Create database schema with proper indexing
    conn.execute_batch(
        r#"
        -- Pools table for current liquidity pool state
        CREATE TABLE IF NOT EXISTS pools (
            pool_id     TEXT PRIMARY KEY,
            token_a     TEXT NOT NULL,
            token_b     TEXT NOT NULL,
            reserve_a   REAL NOT NULL DEFAULT 0.0,
            reserve_b   REAL NOT NULL DEFAULT 0.0,
            last_updated INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_pools_last_updated ON pools(last_updated);

        -- Swaps table for historical transaction data
        CREATE TABLE IF NOT EXISTS swaps (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            pool_id      TEXT NOT NULL,
            amount_in    REAL NOT NULL,
            amount_out   REAL NOT NULL,
            timestamp    INTEGER NOT NULL,
            tx_digest    TEXT NOT NULL UNIQUE  -- Prevents duplicate transaction processing
        );
        CREATE INDEX IF NOT EXISTS idx_swaps_pool_ts ON swaps(pool_id, timestamp DESC);
        "#,
    )?;

    Ok(conn)
}

/// Updates or inserts pool data in the database.
/// 
/// This function uses SQLite's `ON CONFLICT` clause to perform an upsert operation.
/// If a pool with the given `pool_id` already exists, the reserves and timestamp
/// are updated. Otherwise, a new pool record is created.
/// 
/// # Arguments
/// * `conn` - SQLite database connection
/// * `pool_id` - Unique identifier for the pool
/// * `token_a` - Address of the first token in the pair
/// * `token_b` - Address of the second token in the pair
/// * `reserve_a` - Current reserve of token A
/// * `reserve_b` - Current reserve of token B
/// * `last_updated` - Timestamp of the update
/// 
/// # Returns
/// * `Result<()>` - Success or error
pub fn upsert_pool(
    conn: &Connection,
    pool_id: &str,
    token_a: &str,
    token_b: &str,
    reserve_a: f64,
    reserve_b: f64,
    last_updated: i64,
) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO pools (pool_id, token_a, token_b, reserve_a, reserve_b, last_updated)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(pool_id) DO UPDATE SET
            reserve_a = excluded.reserve_a,
            reserve_b = excluded.reserve_b,
            last_updated = excluded.last_updated
        "#,
        params![pool_id, token_a, token_b, reserve_a, reserve_b, last_updated],
    )?;
    Ok(())
}

/// Inserts a swap transaction record if it doesn't already exist.
/// 
/// This function uses `INSERT OR IGNORE` to prevent duplicate transaction
/// processing. The `tx_digest` field has a UNIQUE constraint, so if a
/// transaction with the same digest already exists, the insert is silently
/// ignored.
/// 
/// # Arguments
/// * `conn` - SQLite database connection
/// * `pool_id` - Identifier of the pool where the swap occurred
/// * `amount_in` - Amount of input token swapped
/// * `amount_out` - Amount of output token received
/// * `timestamp` - Transaction timestamp
/// * `tx_digest` - Unique transaction digest for deduplication
/// 
/// # Returns
/// * `Result<()>` - Success or error
pub fn insert_swap(
    conn: &Connection,
    pool_id: &str,
    amount_in: f64,
    amount_out: f64,
    timestamp: i64,
    tx_digest: &str,
) -> Result<()> {
    let _ = conn.execute(
        r#"
        INSERT OR IGNORE INTO swaps (pool_id, amount_in, amount_out, timestamp, tx_digest)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![pool_id, amount_in, amount_out, timestamp, tx_digest],
    )?;
    Ok(())
}
