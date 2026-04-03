use clap::Parser;
use dotenvy::dotenv;
use mongodb::IndexModel;
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use std::collections::HashSet;
use std::process;
use tracing::{Level, error, info, warn};
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
    match dotenv() {
        Ok(path) => info!("Loaded environment variables from {}", path.display()),
        Err(e) if e.not_found() => {
            info!("No .env file found, relying on system environment variables");
        }
        Err(e) => {
            error!("Failed to load .env file: {}", e);
            process::exit(1);
        }
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
            error!("Failed to ensure Atlas Search indexes.");
            error!("Are you running against a standard MongoDB container instead of Atlas?");
            error!("Details: {}", e);
            process::exit(1);
        }
        info!("Atlas Search indexes successfully ensured.");
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

/// Retrieves the names of all existing search indexes on a collection using
/// the `$listSearchIndexes` aggregation stage.
async fn list_existing_search_indexes(
    db: &Db,
    collection: &str,
) -> Result<HashSet<String>, mongodb::error::Error> {
    use futures::TryStreamExt;

    let coll = db.collection::<Document>(collection);

    let pipeline = vec![doc! { "$listSearchIndexes": {} }];
    let mut cursor = coll.aggregate(pipeline).await?;

    let mut names = HashSet::new();
    while let Some(index_doc) = cursor.try_next().await? {
        if let Some(name) = index_doc.get_str("name").ok() {
            names.insert(name.to_string());
        }
    }

    Ok(names)
}

/// Creates a single search index on a collection.
async fn create_search_index(
    db: &Db,
    collection: &str,
    index: Document,
) -> Result<(), mongodb::error::Error> {
    let command = doc! {
        "createSearchIndexes": collection,
        "indexes": [index]
    };
    db.handle().run_command(command).await?;
    Ok(())
}

/// Updates the definition of an existing search index by name.
/// Requires MongoDB 6.0.7+ / Atlas.
async fn update_search_index(
    db: &Db,
    collection: &str,
    name: &str,
    definition: Document,
) -> Result<(), mongodb::error::Error> {
    let command = doc! {
        "updateSearchIndex": collection,
        "name": name,
        "definition": definition
    };
    db.handle().run_command(command).await?;
    Ok(())
}

/// Ensures a single search index exists with the desired definition.
/// If the index already exists it is updated in place; otherwise it is created.
async fn ensure_search_index(
    db: &Db,
    collection: &str,
    existing: &HashSet<String>,
    index: Document,
) -> Result<(), mongodb::error::Error> {
    let name = index
        .get_str("name")
        .expect("search index document must have a 'name' field")
        .to_string();

    let definition = index
        .get_document("definition")
        .expect("search index document must have a 'definition' field")
        .clone();

    if existing.contains(&name) {
        info!("Index '{}' already exists — updating definition...", name);
        update_search_index(db, collection, &name, definition).await?;
        info!(
            "Index '{}' update triggered (rebuilding in background).",
            name
        );
    } else {
        info!("Index '{}' does not exist — creating...", name);
        create_search_index(db, collection, index).await?;
        info!(
            "Index '{}' creation triggered (building in background).",
            name
        );
    }

    Ok(())
}

/// Applies Atlas Search and Vector Search indexes using a create-or-update
/// pattern so the migration is safely rerunnable.
///
/// 1. Lists existing search indexes on the target collection.
/// 2. For each desired index:
///    - If an index with the same name already exists → `updateSearchIndex`
///    - Otherwise → `createSearchIndexes`
async fn apply_search_indexes(db: &Db) -> Result<(), mongodb::error::Error> {
    let existing = match list_existing_search_indexes(db, CHUNKS_COLLECTION).await {
        Ok(names) => {
            if names.is_empty() {
                info!(
                    "No existing search indexes found on '{}'.",
                    CHUNKS_COLLECTION
                );
            } else {
                info!(
                    "Found {} existing search index(es) on '{}': {:?}",
                    names.len(),
                    CHUNKS_COLLECTION,
                    names
                );
            }
            names
        }
        Err(e) => {
            // $listSearchIndexes may fail on non-Atlas deployments.  Warn and
            // fall back to attempting creation (which will fail with a clear
            // error if the index already exists).
            warn!(
                "Could not list existing search indexes ({}). \
                 Falling back to create-only mode.",
                e
            );
            HashSet::new()
        }
    };

    // ── 1. Vector Search Index ───────────────────────────────────────────
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

    ensure_search_index(db, CHUNKS_COLLECTION, &existing, vector_index).await?;

    // ── 2. Full-Text Search Index ────────────────────────────────────────
    //
    // Atlas Search static mappings require nested fields to be declared under
    // a parent field with type "document".  Dot-notation keys like
    // "speaker.name" at the top level of the fields object are not supported
    // and will cause index creation to fail.  Instead, we declare "speaker"
    // as a document containing its own "fields" map.
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
                    "speaker": {
                        "type": "document",
                        "fields": {
                            "name": { "type": "string", "analyzer": "lucene.standard" },
                            "role": { "type": "token" }
                        }
                    },
                    "ticker": { "type": "token" },
                    "year": { "type": "number" },
                    "quarter": { "type": "token" },
                    "sector": { "type": "token" },
                    "chunk_type": { "type": "token" },
                    "call_date": { "type": "date" }
                }
            }
        }
    };

    ensure_search_index(db, CHUNKS_COLLECTION, &existing, text_index).await?;

    Ok(())
}
