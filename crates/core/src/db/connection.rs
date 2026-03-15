use super::config::DbConfig;
use crate::db::error::DbError;
use mongodb::bson::doc;
use mongodb::{Client, Database, options::ClientOptions};

pub struct Db {
    database: Database,
}

impl Db {
    pub async fn connect(config: &DbConfig) -> Result<Self, DbError> {
        let options = ClientOptions::parse(&config.uri)
            .await
            .map_err(|e| DbError::InvalidUri(e.to_string()))?;

        let client =
            Client::with_options(options).map_err(|e| DbError::Connection(e.to_string()))?;

        let database = client.database(&config.database);

        #[cfg(debug_assertions)]
        Self::ping_connection(&database).await?;

        Ok(Self { database })
    }

    pub fn handle(&self) -> &Database {
        &self.database
    }

    #[cfg(debug_assertions)]
    async fn ping_connection(db: &Database) -> Result<(), mongodb::error::Error> {
        db.run_command(doc! { "ping": 1 }).await?;
        Ok(())
    }
}
