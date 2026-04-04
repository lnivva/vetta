use miette::{IntoDiagnostic, Result};

use crate::context::AppContext;
use vetta_core::stt::{LocalSttStrategy, Stt};

pub async fn build_stt(ctx: &AppContext) -> Result<Box<dyn Stt>> {
    let stt = LocalSttStrategy::connect(&ctx.socket)
        .await
        .into_diagnostic()?;

    Ok(Box::new(stt))
}
