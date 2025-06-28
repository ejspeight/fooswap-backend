mod db;
mod indexer;
mod routes;

use axum::{Router, Extension};
use std::sync::{Arc, Mutex};
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Main entry point for the Fooswap DEX backend service.
/// 
/// This application provides:
/// - A blockchain indexer that monitors Sui Move events
/// - A REST API for querying pool and swap data
/// - SQLite-based data persistence
/// 
/// The service runs both the indexer and API server concurrently.
#[tokio::main]
async fn main() {
    // Initialize SQLite database and create schema if needed
    let conn = db::init_db().expect("Failed to initialize database");
    
    // Wrap database connection in thread-safe container for sharing between indexer and API
    let conn_arc = Arc::new(Mutex::new(conn));

    // Start the blockchain indexer as a background task
    // This will continuously poll for new events and update the database
    {
        let conn_for_indexer = conn_arc.clone();
        tokio::spawn(async move {
            indexer::run_indexer(conn_for_indexer).await;
        });
    }

    // Configure the HTTP API routes
    let app = Router::new()
        // Health check endpoint for monitoring and load balancers
        .route("/health", axum::routing::get(|| async { "OK" }))
        // Mount API routes under /api prefix with database connection injection
        .nest(
            "/api",
            routes::api_routes().layer(Extension(conn_arc.clone())),
        );

    // Bind to localhost on port 3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = TcpListener::bind(addr)
        .await
        .expect("Failed to bind to 127.0.0.1:3000");
    println!("Server listening on http://{}", addr);

    // Start the HTTP server
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}
