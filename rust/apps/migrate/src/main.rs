use clap::Parser;
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

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Also create Atlas Search and Vector Search indexes.
    /// Note: This will fail if not running against an Atlas cluster or Atlas Local CLI.
    #[arg(long, default_value_t = false)]
    with_search: bool,
}

#[tokio::main]
async fn main() {
    // 1. Parse CLI arguments
    let args = Args::parse();

    // 2. Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    info!("Starting vetta_migrate database initialization...");

    // 3. Load environment variables
    if dotenv().is_ok() {
        info!("Loaded environment variables from .env file");
    } else {
        info!("No .env file found, relying on system environment variables");
    }

    // 4. Strict Environment Variable Check
    let config = match DbConfig::from_env() {
        Ok(c) => {
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

    // 5. Connect to MongoDB
    info!("Initializing MongoDB client...");
    let db = match Db::connect(&config).await {
        Ok(db) => db,
        Err(e) => {
            error!("Failed to initialize MongoDB client: {}", e);
            process::exit(1);
        }
    };

    // 6. Explicitly Ping the Database
    info!("Pinging database to verify connection...");
    match db.handle().run_command(doc! { "ping": 1 }).await {
        Ok(_) => info!("Database connection verified successfully."),
        Err(e) => {
            error!("Database ping failed. Is the server running and accessible?");
            error!("Details: {}", e);
            process::exit(1);
        }
    }

    // 7. Run the B-Tree index migrations
    info!("Ensuring standard B-Tree indexes exist on collections...");
    if let Err(e) = apply_standard_indexes(&db).await {
        error!("Failed to create standard indexes: {}", e);
        process::exit(1);
    }
    info!("Standard indexes successfully verified/created.");

    // 8. Conditionally run Atlas Search index migrations
    if args.with_search {
        info!("Ensuring Atlas Search and Vector Search indexes...");
        if let Err(e) = apply_search_indexes(&db).await {
            error!("Failed to create Atlas Search indexes.");
            error!("Are you running against a standard MongoDB container instead of Atlas?");
            error!("Details: {}", e);
            process::exit(1);
        }
        info!("Atlas Search index creation triggered (building in background).");
    } else {
        info!("Skipping Atlas Search indexes. Use --with-search to apply them.");
    }

    info!("Database migration completed successfully.");
}

/// Applies all required standard MongoDB B-Tree indexes to the database.
async fn apply_standard_indexes(db: &Db) -> Result<(), mongodb::error::Error> {
    let calls = db.collection::<EarningsCallDocument>(CALLS_COLLECTION);
    let chunks = db.collection::<EarningsChunkDocument>(CHUNKS_COLLECTION);

    // --- earnings_calls indexes ---
    calls
        .create_index(
            IndexModel::builder()
                .keys(doc! { "ticker": 1, "year": 1, "quarter": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await?;

    calls
        .create_index(IndexModel::builder().keys(doc! { "call_date": -1 }).build())
        .await?;

    calls
        .create_index(
            IndexModel::builder()
                .keys(doc! { "company.sector": 1, "call_date": -1 })
                .build(),
        )
        .await?;

    calls
        .create_index(
            IndexModel::builder()
                .keys(doc! { "status": 1, "updated_at": -1 })
                .build(),
        )
        .await?;

    // --- earnings_chunks indexes ---
    chunks
        .create_index(
            IndexModel::builder()
                .keys(doc! { "call_id": 1, "chunk_index": 1 })
                .build(),
        )
        .await?;

    chunks
        .create_index(
            IndexModel::builder()
                .keys(doc! { "ticker": 1, "call_date": -1 })
                .build(),
        )
        .await?;

    chunks
        .create_index(
            IndexModel::builder()
                .keys(doc! { "model_version": 1 })
                .build(),
        )
        .await?;

    Ok(())
}

/// Applies Atlas Search and Vector Search indexes using raw database commands.
async fn apply_search_indexes(db: &Db) -> Result<(), mongodb::error::Error> {
    // 1. Define the Vector Search Index
    let vector_index = doc! {
        "name": "chunk_vector_index",
        "type": "vectorSearch",
        "definition": {
            "fields": [
                { "path": "embedding", "type": "vector", "numDimensions": 1024, "similarity": "cosine" },
                { "path": "ticker", "type": "filter" },
                { "path": "year", "type": "filter" },
                { "path": "quarter", "type": "filter" },
                { "path": "sector", "type": "filter" },
                { "path": "chunk_type", "type": "filter" },
                { "path": "speaker.role", "type": "filter" },
                { "path": "call_date", "type": "filter" }
            ]
        }
    };

    // 2. Define the Full-Text Search Index
    let text_index = doc! {
        "name": "chunk_text_index",
        "type": "search",
        "definition": {
            "analyzer": "lucene.english",
            "mappings": {
                "dynamic": false,
                "fields": {
                    "text": {
                        "type": "string",
                        "analyzer": "lucene.english",
                        "multi": {
                            "keyword": { "type": "string", "analyzer": "lucene.keyword" }
                        }
                    },
                    "speaker.name": { "type": "string", "analyzer": "lucene.standard" },
                    "ticker": { "type": "token" },
                    "year": { "type": "number" },
                    "quarter": { "type": "token" },
                    "sector": { "type": "token" },
                    "chunk_type": { "type": "token" },
                    "speaker.role": { "type": "token" },
                    "call_date": { "type": "date" }
                }
            }
        }
    };

    // Construct the actual command payload
    let command = doc! {
        "createSearchIndexes": CHUNKS_COLLECTION,
        "indexes": [vector_index, text_index]
    };

    // Execute the command against the database
    db.handle().run_command(command).await?;

    Ok(())
}
