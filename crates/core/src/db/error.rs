use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum DbError {
    #[error("Failed to connect to MongoDB: {0}")]
    #[diagnostic(help("Is MongoDB running? Check the connection URI in config.toml"))]
    Connection(String),

    #[error("Failed to parse MongoDB connection string: {0}")]
    #[diagnostic(help("Check the URI format in config.toml: mongodb://host:port"))]
    InvalidUri(String),

    #[error("Query failed: {0}")]
    QueryFailure(String),

    #[error("Document not found: {0}")]
    NotFound(String),

    #[error("Failed to serialize/deserialize document: {0}")]
    #[diagnostic(help("Check that your struct fields match the MongoDB document schema"))]
    Serialization(String),

    #[error("Duplicate document: {0}")]
    #[diagnostic(help("A document with this key already exists"))]
    Duplicate(String),

    #[error("Bulk write failed: {success} succeeded, {failure} failed")]
    BulkWrite { success: u64, failure: u64 },
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
            _ => DbError::QueryFailure(err.to_string()),
        }
    }
}
