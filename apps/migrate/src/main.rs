use dotenvy::dotenv;
use mongodb::IndexModel;
use mongodb::bson::doc;
use mongodb::options::IndexOptions;
use std::process;
use tracing::{Level, error, info};
use tracing_subscriber::FmtSubscriber;

use vetta_core::db::models::{EarningsCallDocument, EarningsChunkDocument};
use vetta_core::db::{Db, DbConfig};

const CALLS_COLLECTION: &str = "earnings_calls";
const CHUNKS_COLLECTION: &str = "earnings_chunks";

#[tokio::main]
async fn main() {
    // 1. Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    info!("Starting vetta_migrate database initialization...");

    // 2. Load environment variables (useful for local development)
    if dotenv().is_ok() {
        info!("Loaded environment variables from .env file");
    } else {
        info!("No .env file found, relying on system environment variables");
    }

    // 3. Strict Environment Variable Check
    let config = match DbConfig::from_env() {
        Ok(c) => {
            // Log safely (don't print passwords in CI/CD logs)
            let safe_uri = if c.uri.contains('@') {
                "mongodb://***@***".to_string()
            } else {
                c.uri.clone()
            };
            info!(
                "Environment OK. Target Database: '{}', URI: {}",
                c.database, safe_uri
            );
            c
        }
        Err(e) => {
            error!("Missing required environment variables: {}", e);
            error!("Please ensure MONGODB_URI and MONGODB_DATABASE are set.");
            process::exit(1);
        }
    };

    // 4. Connect to MongoDB
    info!("Initializing MongoDB client...");
    let db = match Db::connect(&config).await {
        Ok(db) => db,
        Err(e) => {
            error!("Failed to initialize MongoDB client: {}", e);
            process::exit(1);
        }
    };

    // 5. Explicitly Ping the Database (Pre-flight check)
    info!("Pinging database to verify connection...");
    match db.handle().run_command(doc! { "ping": 1 }).await {
        Ok(_) => info!("Database connection verified successfully."),
        Err(e) => {
            error!("Database ping failed. Is the server running and accessible?");
            error!("Details: {}", e);
            process::exit(1);
        }
    }

    // 6. Run the B-Tree index migrations
    info!("Ensuring standard B-Tree indexes exist on collections...");
    if let Err(e) = apply_indexes(&db).await {
        error!("Failed to create indexes: {}", e);
        process::exit(1);
    }

    // Reminder for Atlas-specific indexes
    info!(
        "Note: Vector Search and Full-Text Search indexes should be applied via IaC (Terraform)."
    );
    info!("Database migration completed successfully.");
}

/// Applies all required standard MongoDB B-Tree indexes to the database.
async fn apply_indexes(db: &Db) -> Result<(), mongodb::error::Error> {
    let calls = db.collection::<EarningsCallDocument>(CALLS_COLLECTION);
    let chunks = db.collection::<EarningsChunkDocument>(CHUNKS_COLLECTION);

    // --- earnings_calls indexes ---

    // Unique business key
    calls
        .create_index(
            IndexModel::builder()
                .keys(doc! { "ticker": 1, "year": 1, "quarter": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await?;

    // Temporal queries
    calls
        .create_index(IndexModel::builder().keys(doc! { "call_date": -1 }).build())
        .await?;

    // Sector temporal queries
    calls
        .create_index(
            IndexModel::builder()
                .keys(doc! { "company.sector": 1, "call_date": -1 })
                .build(),
        )
        .await?;

    // Pipeline status tracking
    calls
        .create_index(
            IndexModel::builder()
                .keys(doc! { "status": 1, "updated_at": -1 })
                .build(),
        )
        .await?;

    // --- earnings_chunks indexes ---

    // Parent reference and chunk ordering
    chunks
        .create_index(
            IndexModel::builder()
                .keys(doc! { "call_id": 1, "chunk_index": 1 })
                .build(),
        )
        .await?;

    // Denormalized temporal queries
    chunks
        .create_index(
            IndexModel::builder()
                .keys(doc! { "ticker": 1, "call_date": -1 })
                .build(),
        )
        .await?;

    // Embedding model tracking (for pipeline upgrades)
    chunks
        .create_index(
            IndexModel::builder()
                .keys(doc! { "model_version": 1 })
                .build(),
        )
        .await?;

    Ok(())
}
