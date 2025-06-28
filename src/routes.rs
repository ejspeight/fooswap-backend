use axum::{
    extract::{Path, Query, Extension},
    routing::get,
    Router,
    response::Json,
};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

/// Represents a liquidity pool in the DEX
#[derive(Serialize)]
struct PoolInfo {
    pool_id: String,
    token_a: String,
    token_b: String,
    reserve_a: f64,
    reserve_b: f64,
    last_updated: i64,
}

/// Represents a swap transaction in the DEX
#[derive(Serialize)]
struct SwapInfo {
    pool_id: String,
    amount_in: f64,
    amount_out: f64,
    timestamp: i64,
}

/// Retrieves all liquidity pools from the database.
/// 
/// Returns a JSON response containing an array of pool information including
/// current reserves, token addresses, and last update timestamp.
/// 
/// # Endpoint
/// `GET /api/pools`
/// 
/// # Response Format
/// ```json
/// {
///   "status": "ok",
///   "data": [
///     {
///       "pool_id": "0x...",
///       "token_a": "0x...",
///       "token_b": "0x...",
///       "reserve_a": 1000.0,
///       "reserve_b": 500.0,
///       "last_updated": 1751104133893
///     }
///   ]
/// }
/// ```
async fn pools_handler(
    Extension(conn_arc): Extension<Arc<Mutex<Connection>>>,
) -> Json<serde_json::Value> {
    // Acquire database connection lock
    let conn = conn_arc.lock().unwrap();

    // Prepare SQL query to fetch all pools
    let mut stmt = conn
        .prepare(
            "SELECT pool_id, token_a, token_b, reserve_a, reserve_b, last_updated
             FROM pools",
        )
        .unwrap();

    // Execute query and map results to PoolInfo structs
    let rows = stmt
        .query_map([], |row| {
            Ok(PoolInfo {
                pool_id: row.get(0)?,
                token_a: row.get(1)?,
                token_b: row.get(2)?,
                reserve_a: row.get(3)?,
                reserve_b: row.get(4)?,
                last_updated: row.get(5)?,
            })
        })
        .unwrap();

    // Collect all pool data into a vector
    let mut pools = Vec::new();
    for r in rows {
        pools.push(r.unwrap());
    }

    Json(json!({ "status": "ok", "data": pools }))
}

/// Retrieves recent swap history for a specific pool.
/// 
/// Returns the last 20 swap transactions for the specified pool, ordered by
/// timestamp in descending order (most recent first).
/// 
/// # Endpoint
/// `GET /api/swaps/{pool_id}`
/// 
/// # Parameters
/// * `pool_id` - The unique identifier of the liquidity pool
/// 
/// # Response Format
/// ```json
/// {
///   "status": "ok",
///   "data": [
///     {
///       "pool_id": "0x...",
///       "amount_in": 100.0,
///       "amount_out": 50.0,
///       "timestamp": 1751104259632
///     }
///   ]
/// }
/// ```
async fn swaps_handler(
    Path(pool_id): Path<String>,
    Extension(conn_arc): Extension<Arc<Mutex<Connection>>>,
) -> Json<serde_json::Value> {
    let conn = conn_arc.lock().unwrap();

    // Prepare SQL query to fetch recent swaps for the specified pool
    let mut stmt = conn
        .prepare(
            "SELECT amount_in, amount_out, timestamp
             FROM swaps
             WHERE pool_id = ?1
             ORDER BY timestamp DESC
             LIMIT 20",
        )
        .unwrap();

    // Execute query and map results to SwapInfo structs
    let rows = stmt
        .query_map([pool_id.clone()], |row| {
            Ok(SwapInfo {
                pool_id: pool_id.clone(),
                amount_in: row.get(0)?,
                amount_out: row.get(1)?,
                timestamp: row.get(2)?,
            })
        })
        .unwrap();

    // Collect all swap data into a vector
    let mut swaps = Vec::new();
    for s in rows {
        swaps.push(s.unwrap());
    }

    Json(json!({ "status": "ok", "data": swaps }))
}

/// Calculates the current price for a token pair based on pool reserves.
/// 
/// Uses the constant product formula (x * y = k) to calculate the price
/// of token B in terms of token A from the current pool reserves.
/// 
/// # Endpoint
/// `GET /api/price?pair=TOKENA/TOKENB`
/// 
/// # Query Parameters
/// * `pair` - Token pair in format "TOKENA/TOKENB" (e.g., "USDC/SUI")
/// 
/// # Response Format
/// ```json
/// {
///   "status": "ok",
///   "pair": "USDC/SUI",
///   "pool_id": "0x...",
///   "price": 0.5
/// }
/// ```
async fn price_handler(
    Query(params): Query<HashMap<String, String>>,
    Extension(conn_arc): Extension<Arc<Mutex<Connection>>>,
) -> Json<serde_json::Value> {
    let conn = conn_arc.lock().unwrap();

    // Extract and validate the pair parameter
    let pair = match params.get("pair") {
        Some(p) => p.clone(),
        None => {
            return Json(json!({
                "status": "error",
                "message": "Missing `pair` query parameter"
            }));
        }
    };

    // Parse token symbols from the pair string
    let tokens: Vec<&str> = pair.split('/').collect();
    if tokens.len() != 2 {
        return Json(json!({
            "status": "error",
            "message": "Query parameter `pair` must be in the form TOKENA/TOKENB"
        }));
    }
    let (token_a, token_b) = (tokens[0], tokens[1]);

    // Query database for the specified token pair
    let mut stmt = conn
        .prepare(
            "SELECT pool_id, reserve_a, reserve_b
             FROM pools
             WHERE token_a = ?1 AND token_b = ?2
             LIMIT 1",
        )
        .unwrap();

    let mut rows = stmt
        .query_map([token_a, token_b], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?, row.get::<_, f64>(2)?))
        })
        .unwrap();

    // Calculate price from reserves if pool exists
    if let Some(Ok((pool_id, reserve_a, reserve_b))) = rows.next() {
        let price = if reserve_a > 0.0 {
            reserve_b / reserve_a
        } else {
            0.0
        };
        Json(json!({
            "status": "ok",
            "pair": pair,
            "pool_id": pool_id,
            "price": price
        }))
    } else {
        Json(json!({
            "status": "error",
            "message": format!("No pool found for {}", pair)
        }))
    }
}

/// Creates and returns the API router with all DEX endpoints.
/// 
/// This function configures all the HTTP routes for the DEX API,
/// including pools, swaps, and price calculation endpoints.
/// 
/// # Returns
/// * `Router` - Axum router configured with all API routes
pub fn api_routes() -> Router {
    Router::new()
        .route("/pools", get(pools_handler))
        .route("/swaps/:pool_id", get(swaps_handler))
        .route("/price", get(price_handler))
}
