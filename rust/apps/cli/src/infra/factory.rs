use crate::context::AppContext;
use miette::{IntoDiagnostic, Result};
use vetta_core::stt::{LocalSttStrategy, Stt};
use vetta_core::{Embedder, LocalEmbeddingsStrategy};

pub async fn build_stt(ctx: &AppContext) -> Result<Box<dyn Stt>> {
    let stt = LocalSttStrategy::connect(&ctx.config.ai_grpc_service_socket_path)
        .await
        .into_diagnostic()?;

    Ok(Box::new(stt))
}

pub async fn build_embedder(ctx: &AppContext) -> Result<Box<dyn Embedder>> {
    let embedder = LocalEmbeddingsStrategy::connect(&ctx.config.ai_grpc_service_socket_path)
        .await
        .into_diagnostic()?;

    Ok(Box::new(embedder))
}
