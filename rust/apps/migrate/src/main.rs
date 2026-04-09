use clap::Parser;
use dotenvy::dotenv;
use miette::{IntoDiagnostic, Result, WrapErr};
use mongodb::IndexModel;
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use std::collections::HashSet;
use std::io;
use tracing::{Level, debug, info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

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
async fn main() -> Result<()> {
    // 1. Parse CLI arguments
    let args = Args::parse();

    // 2. Initialize logging to stderr
    let subscriber = FmtSubscriber::builder()
        .with_writer(io::stderr)
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(Level::INFO.into())
                .from_env_lossy(),
        )
        .finish();

    tracing::subscriber::set_global_default(subscriber).into_diagnostic()?;

    debug!("Starting vetta_migrate database initialization...");

    // 3. Load environment variables
    if let Ok(env_path) = std::env::var("VETTA_ENV_PATH") {
        dotenvy::from_filename(&env_path)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to load inherited .env file at {}", env_path))?;
        debug!("Loaded context from inherited VETTA_ENV_PATH: {}", env_path);
    } else {
        match dotenv() {
            Ok(path) => debug!("Loaded environment variables from local {}", path.display()),
            Err(e) if e.not_found() => {
                debug!("No .env file found, relying on system environment variables")
            }
            Err(e) => {
                return Err(e)
                    .into_diagnostic()
                    .wrap_err("Failed to load local .env file");
            }
        }
    }

    // 4. Strict Environment Variable Check
    let config = DbConfig::from_env()
        .into_diagnostic()
        .wrap_err("Missing required environment variables. Please ensure MONGODB_URI and MONGODB_DATABASE are set.")?;

    let safe_uri = if config.uri.contains('@') {
        "mongodb://***@***".to_string()
    } else {
        config.uri.clone()
    };
    debug!(
        "Environment OK. Target Database: '{}', URI: {}",
        config.database, safe_uri
    );

    // 5. Connect to MongoDB
    debug!("Initializing MongoDB client...");
    let db = Db::connect(&config)
        .await
        .into_diagnostic()
        .wrap_err("Failed to initialize MongoDB client")?;

    // 6. Explicitly Ping the Database
    debug!("Pinging database to verify connection...");
    db.handle()
        .run_command(doc! { "ping": 1 })
        .await
        .into_diagnostic()
        .wrap_err("Database ping failed. Is the server running and accessible?")?;
    debug!("Database connection verified successfully.");

    // 7. Run the B-Tree index migrations
    debug!("Ensuring standard B-Tree indexes exist on collections...");
    apply_standard_indexes(&db)
        .await
        .wrap_err("Failed to create standard indexes")?;
    debug!("Standard indexes successfully verified/created.");

    // 8. Conditionally run Atlas Search index migrations
    if args.with_search {
        debug!("Ensuring Atlas Search and Vector Search indexes...");
        apply_search_indexes(&db)
            .await
            .wrap_err("Failed to ensure Atlas Search indexes. Are you running against a standard MongoDB container instead of Atlas?")?;
        debug!("Atlas Search indexes successfully ensured.");
    } else {
        debug!("Skipping Atlas Search indexes. Use --with-search to apply them.");
    }

    debug!("Database migration completed successfully.");
    Ok(())
}

/// Applies all required standard MongoDB B-Tree indexes to the database.
async fn apply_standard_indexes(db: &Db) -> Result<()> {
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
        .await
        .into_diagnostic()?;

    calls
        .create_index(IndexModel::builder().keys(doc! { "call_date": -1 }).build())
        .await
        .into_diagnostic()?;

    calls
        .create_index(
            IndexModel::builder()
                .keys(doc! { "company.sector": 1, "call_date": -1 })
                .build(),
        )
        .await
        .into_diagnostic()?;

    calls
        .create_index(
            IndexModel::builder()
                .keys(doc! { "status": 1, "updated_at": -1 })
                .build(),
        )
        .await
        .into_diagnostic()?;

    // --- earnings_chunks indexes ---
    chunks
        .create_index(
            IndexModel::builder()
                .keys(doc! { "call_id": 1, "chunk_index": 1 })
                .build(),
        )
        .await
        .into_diagnostic()?;

    chunks
        .create_index(
            IndexModel::builder()
                .keys(doc! { "ticker": 1, "call_date": -1 })
                .build(),
        )
        .await
        .into_diagnostic()?;

    chunks
        .create_index(
            IndexModel::builder()
                .keys(doc! { "model_version": 1 })
                .build(),
        )
        .await
        .into_diagnostic()?;

    Ok(())
}

/// Retrieves the names of all existing search indexes on a collection using
/// the `$listSearchIndexes` aggregation stage.
async fn list_existing_search_indexes(db: &Db, collection: &str) -> Result<HashSet<String>> {
    use futures::TryStreamExt;

    let coll = db.collection::<Document>(collection);

    let pipeline = vec![doc! { "$listSearchIndexes": {} }];
    let mut cursor = coll.aggregate(pipeline).await.into_diagnostic()?;

    let mut names = HashSet::new();
    while let Some(index_doc) = cursor.try_next().await.into_diagnostic()? {
        if let Ok(name) = index_doc.get_str("name") {
            names.insert(name.to_string());
        }
    }

    Ok(names)
}

/// Creates a single search index on a collection.
async fn create_search_index(db: &Db, collection: &str, index: Document) -> Result<()> {
    let command = doc! {
        "createSearchIndexes": collection,
        "indexes": [index]
    };
    db.handle().run_command(command).await.into_diagnostic()?;
    Ok(())
}

/// Updates the definition of an existing search index by name.
/// Requires MongoDB 6.0.7+ / Atlas.
async fn update_search_index(
    db: &Db,
    collection: &str,
    name: &str,
    definition: Document,
) -> Result<()> {
    let command = doc! {
        "updateSearchIndex": collection,
        "name": name,
        "definition": definition
    };
    db.handle().run_command(command).await.into_diagnostic()?;
    Ok(())
}

/// Ensures a single search index exists with the desired definition.
/// If the index already exists it is updated in place; otherwise it is created.
async fn ensure_search_index(
    db: &Db,
    collection: &str,
    existing: &HashSet<String>,
    index: Document,
) -> Result<()> {
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
async fn apply_search_indexes(db: &Db) -> Result<()> {
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
