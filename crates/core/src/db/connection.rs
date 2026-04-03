use super::config::DbConfig;
use crate::db::DbError;
use mongodb::bson::doc;
use mongodb::{Client, Collection, Database, options::ClientOptions};
use serde::{Deserialize, Serialize};

pub struct Db {
    client: Client,
    database: Database,
    supports_transactions: bool,
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

        let supports_transactions = Self::check_transaction_support(&database).await;

        Ok(Self {
            client,
            database,
            supports_transactions,
        })
    }

    pub fn handle(&self) -> &Database {
        &self.database
    }

    /// Get the underlying client.  
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Whether the connected cluster supports multi-document transactions.  
    pub fn supports_transactions(&self) -> bool {
        self.supports_transactions
    }

    /// Get a typed collection handle.  
    pub fn collection<T>(&self, name: &str) -> Collection<T>
    where
        T: Serialize + for<'de> Deserialize<'de> + Unpin + Send + Sync,
    {
        self.database.collection::<T>(name)
    }

    /// Check whether the server is a replica set or sharded cluster
    async fn check_transaction_support(db: &Database) -> bool {
        let result = db.run_command(doc! { "hello": 1 }).await;

        match result {
            Ok(doc) => {
                // Replica set members expose `setName`
                let is_replica_set = doc.get_str("setName").is_ok();
                // Mongos routers return msg: "isdbgrid"
                let is_sharded = doc.get_str("msg").map(|m| m == "isdbgrid").unwrap_or(false);

                is_replica_set || is_sharded
            }
            Err(_) => false,
        }
    }

    #[cfg(debug_assertions)]
    async fn ping_connection(db: &Database) -> Result<(), mongodb::error::Error> {
        db.run_command(doc! { "ping": 1 }).await?;
        Ok(())
    }
}
