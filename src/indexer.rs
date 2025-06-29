use rusqlite::Connection;
use serde_json::Value;
use std::{sync::Arc, sync::Mutex, time::{SystemTime, UNIX_EPOCH}};
use tokio::time::sleep;
use std::time::Duration;
use crate::db::{upsert_pool, insert_swap};

/// Interval between polling cycles for new blockchain events (in seconds)
const POLL_INTERVAL_SECS: u64 = 5;

/// Sui Move package ID for the Fooswap DEX contract
/// This should be updated when deploying to different networks (devnet, testnet, mainnet)
const DEX_PACKAGE_ID: &str = "0x1c2be4cfbf91fe8d71aedeb83cbe680475b70359bab87900df99ecd787ca5474";

/// Queries Sui blockchain for DEX events within a specified time range.
/// 
/// This function fetches both PoolCreatedEvent and SwapEvent types from the Sui RPC
/// using the `suix_queryEvents` method. Events are retrieved in batches of 100.
/// 
/// # Arguments
/// * `from_ts` - Start timestamp (inclusive) in milliseconds since epoch
/// * `to_ts` - End timestamp (exclusive) in milliseconds since epoch
/// 
/// # Returns
/// * `Result<Vec<serde_json::Value>>` - Vector of event JSON objects or error
async fn query_sui_events(
    from_ts: i64,
    to_ts: i64,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    let rpc_url = std::env::var("SUI_RPC_URL")
        .unwrap_or_else(|_| "https://fullnode.devnet.sui.io:443".to_string());
    let client = reqwest::Client::new();
    let mut all_events = Vec::new();
    
    // Define the event types to query from the Sui Move contract
    let event_types = [
        format!("{}::fooswap::PoolCreatedEvent", DEX_PACKAGE_ID),
        format!("{}::fooswap::SwapEvent", DEX_PACKAGE_ID),
    ];
    
    for event_type in event_types.iter() {
        // Use timestamp-based filtering to avoid fetching duplicate events
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "suix_queryEvents",
            "params": [
                { "MoveEventType": event_type },
                null,  // cursor (null for latest)
                100,   // limit
                false, // descending order
                {      // time range filter
                    "TimeRange": {
                        "start_time": from_ts,
                        "end_time": to_ts
                    }
                }
            ]
        });
        
        println!("Querying Sui RPC: {}", rpc_url);
        println!("Request body: {}", serde_json::to_string_pretty(&request_body).unwrap());
        
        let resp = client
            .post(&rpc_url)
            .json(&request_body)
            .send()
            .await?;
            
        if !resp.status().is_success() {
            return Err(format!("Sui RPC returned error status: {}", resp.status()).into());
        }
        
        let json: serde_json::Value = resp.json().await?;
        println!("Response: {}", serde_json::to_string_pretty(&json).unwrap());
        
        // Extract events from the RPC response
        if let Some(data) = json.get("result").and_then(|r| r.get("data")).and_then(|d| d.as_array()) {
            for event in data {
                all_events.push(event.clone());
            }
        }
    }
    Ok(all_events)
}

/// Processes blockchain events and persists them to the local SQLite database.
/// 
/// This function parses Sui Move events from the JSON-RPC response format and
/// extracts relevant data for pool creation and swap operations. Each event
/// type is handled differently based on the Move contract's event structure.
/// 
/// # Arguments
/// * `conn` - SQLite database connection
/// * `events` - Array of event JSON objects from Sui RPC
fn process_events(conn: &Connection, events: &[Value]) {
    for evt in events {
        // Sui event structure:
        // {
        //   "id": { "txDigest": "0x...", "eventSeq": "0" },
        //   "parsedJson": { "creator": "...", "pool_id": "...", ... },
        //   "timestampMs": "1751104133893",
        //   "type": "0x...::fooswap::PoolCreatedEvent" OR "0x...::fooswap::SwapEvent",
        //   ...
        // }
        let parsed = &evt["parsedJson"];
        let ts = evt["timestampMs"].as_str().unwrap_or("0").parse::<i64>().unwrap_or(0);
        let tx_digest = evt["id"]["txDigest"].as_str().unwrap_or_default();
        let event_type = evt["type"].as_str().unwrap_or_default();

        if event_type.contains("PoolCreatedEvent") {
            // Extract pool creation event data
            let pool_id = parsed["pool_id"].as_str().unwrap_or_default();
            let token_a = parsed["token_a"].as_str().unwrap_or_default();
            let token_b = parsed["token_b"].as_str().unwrap_or_default();
            let initial_reserve_a = parsed["initial_reserve_a"]
                .as_str()
                .unwrap_or("0")
                .parse::<f64>()
                .unwrap_or(0.0);
            let initial_reserve_b = parsed["initial_reserve_b"]
                .as_str()
                .unwrap_or("0")
                .parse::<f64>()
                .unwrap_or(0.0);

            println!("Processing PoolCreatedEvent: pool_id={}, token_a={}, token_b={}, reserve_a={}, reserve_b={}", 
                     pool_id, token_a, token_b, initial_reserve_a, initial_reserve_b);

            // Persist pool data to database
            let _ = upsert_pool(
                conn,
                pool_id,
                token_a,
                token_b,
                initial_reserve_a,
                initial_reserve_b,
                ts,
            );
        }
        else if event_type.contains("SwapEvent") {
            // Extract swap event data
            let pool_id = parsed["pool_id"].as_str().unwrap_or_default();
            let amount_in = parsed["amount_in"]
                .as_str()
                .unwrap_or("0")
                .parse::<f64>()
                .unwrap_or(0.0);
            let amount_out = parsed["amount_out"]
                .as_str()
                .unwrap_or("0")
                .parse::<f64>()
                .unwrap_or(0.0);

            // Extract updated reserves after the swap
            let new_reserve_a = parsed["new_reserve_a"]
                .as_str()
                .unwrap_or("0")
                .parse::<f64>()
                .unwrap_or(0.0);
            let new_reserve_b = parsed["new_reserve_b"]
                .as_str()
                .unwrap_or("0")
                .parse::<f64>()
                .unwrap_or(0.0);

            println!("Processing SwapEvent: pool_id={}, amount_in={}, amount_out={}, new_reserve_a={}, new_reserve_b={}", 
                     pool_id, amount_in, amount_out, new_reserve_a, new_reserve_b);

            // Record the swap transaction
            let _ = insert_swap(conn, pool_id, amount_in, amount_out, ts, tx_digest);

            // Update pool reserves to reflect the swap
            let _ = upsert_pool(conn, pool_id, "", "", new_reserve_a, new_reserve_b, ts);
        }
    }
}

/// Runs the blockchain indexer as a continuous background process.
/// 
/// This function implements a polling-based indexer that continuously monitors
/// the Sui blockchain for new DEX events. It maintains a timestamp-based cursor
/// to avoid reprocessing events and persists all events to the local SQLite database.
/// 
/// The indexer runs indefinitely until the process is terminated. It polls the
/// blockchain every `POLL_INTERVAL_SECS` seconds and processes any new events found.
/// 
/// # Arguments
/// * `conn_arc` - Thread-safe SQLite connection wrapped in Arc<Mutex<Connection>>
pub async fn run_indexer(conn_arc: Arc<Mutex<Connection>>) {
    // Initialize cursor to genesis (timestamp 0)
    let mut last_ts: i64 = 0;

    loop {
        // Calculate current timestamp for the polling window
        let to_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        println!("Indexer polling: searching for events from {} to {}", last_ts, to_ts);

        // Query blockchain for events in the time range [last_ts, to_ts)
        match query_sui_events(last_ts, to_ts).await {
            Ok(events) => {
                if !events.is_empty() {
                    println!("Found {} new events, processing...", events.len());
                    if let Ok(conn) = conn_arc.lock() {
                        process_events(&conn, &events);
                    }
                    last_ts = to_ts;
                } else {
                    println!("No new events found in time range");
                }
            }
            Err(e) => {
                eprintln!("Warning: failed to query Sui events: {}", e);
            }
        }

        // Wait before the next polling cycle
        sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
    }
}
