use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct DbConfig {
    pub uri: String,
    pub database: String,
}

impl DbConfig {
    pub fn new(uri: impl Into<String>, database: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            database: database.into(),
        }
    }

    pub fn from_env() -> Result<Self, std::env::VarError> {
        Ok(Self {
            uri: std::env::var("MONGODB_URI")?,
            database: std::env::var("MONGODB_DATABASE")?,
        })
    }
}
