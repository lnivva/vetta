use miette::Diagnostic;
use thiserror::Error;

pub mod db;
pub mod domain;
pub mod earnings_processor;
pub mod stt;

#[derive(Debug, Error, Diagnostic)]
pub enum AppError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Db(#[from] db::DbError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Stt(#[from] stt::SttError),
}
