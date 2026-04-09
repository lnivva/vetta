//! Earnings-call ingestion pipeline.  
//!  
//! This module owns the full ingest → transcribe → store workflow and exposes  
//! a trait-based observer API so any frontend (CLI, desktop, server) can  
//! react to progress without coupling to a specific UI.  

mod errors;
mod events;
mod utils;
mod processor;

pub use errors::{EarningsError, IngestError};
pub use events::{EarningsEvent, EarningsObserver, NullObserver};
pub use ingest::{validate_media_file, ALLOWED_MIME_TYPES, MAX_FILE_SIZE_MB};
pub use processor::{EarningsProcessor, ProcessRequest};  
