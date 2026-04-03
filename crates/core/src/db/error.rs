use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum DbError {
    #[error("Failed to connect to MongoDB: {0}")]
    #[diagnostic(
        code(vetta::db::connection),
        help("Is MongoDB running? Check the connection URI in config.toml")
    )]
    Connection(String),

    #[error("Failed to parse MongoDB connection string: {0}")]
    #[diagnostic(
        code(vetta::db::invalid_uri),
        help("Check the URI format in config.toml: mongodb+srv://user:pass@host/db")
    )]
    InvalidUri(String),

    #[error("Query failed: {0}")]
    #[diagnostic(code(vetta::db::query))]
    QueryFailure(String),

    #[error("Document not found: {0}")]
    #[diagnostic(code(vetta::db::not_found))]
    NotFound(String),

    #[error("Failed to serialize/deserialize document: {0}")]
    #[diagnostic(
        code(vetta::db::serialization),
        help("Check that your struct fields match the MongoDB document schema")
    )]
    Serialization(String),

    #[error("Duplicate document: {0}")]
    #[diagnostic(
        code(vetta::db::duplicate),
        help("A document with this key already exists. Use --replace to overwrite.")
    )]
    Duplicate(String),

    #[error("Bulk write failed: {success} succeeded, {failure} failed")]
    #[diagnostic(code(vetta::db::bulk_write))]
    BulkWrite { success: u64, failure: u64 },

    #[error("Transactions not supported on this cluster tier")]
    #[diagnostic(
        code(vetta::db::transactions_unsupported),
        help(
            "Your Atlas cluster (M0/M2/M5) does not support multi-document transactions. Vetta will use direct writes automatically. Upgrade to M10+ for full transaction support."
        )
    )]
    TransactionNotSupported,
}

impl From<mongodb::error::Error> for DbError {
    fn from(err: mongodb::error::Error) -> Self {
        use mongodb::error::ErrorKind;

        match *err.kind {
            ErrorKind::InvalidArgument { .. } => DbError::InvalidUri(err.to_string()),

            ErrorKind::Authentication { .. } => {
                DbError::Connection(format!("Authentication failed: {err}"))
            }

            ErrorKind::BsonDeserialization(_) | ErrorKind::BsonSerialization(_) => {
                DbError::Serialization(err.to_string())
            }

            ErrorKind::Write(ref _write_err) => {
                let msg = err.to_string();
                if msg.contains("E11000") || msg.contains("duplicate key") {
                    DbError::Duplicate(msg)
                } else {
                    DbError::QueryFailure(msg)
                }
            }

            ErrorKind::Transaction { .. } => DbError::TransactionNotSupported,

            ErrorKind::ServerSelection { .. } => {
                DbError::Connection(format!("Cannot reach cluster: {err}"))
            }

            ErrorKind::DnsResolve { .. } => {
                DbError::Connection(format!("DNS resolution failed: {err}"))
            }

            ErrorKind::Io(ref _inner) => DbError::Connection(format!("Network error: {err}")),

            ErrorKind::ConnectionPoolCleared { .. } => {
                DbError::Connection(format!("Connection pool reset: {err}"))
            }

            _ => DbError::QueryFailure(err.to_string()),
        }
    }
}
