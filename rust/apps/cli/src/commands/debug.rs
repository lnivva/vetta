use clap::Subcommand;
use miette::{IntoDiagnostic, Result};

use crate::{context::AppContext, infra::factory};
use vetta_core::db::{Db, DbConfig};
use vetta_core::{InputType, SearchFilters, build_searcher};

#[derive(Subcommand)]
pub enum DebugAction {
    SearchVectors {
        #[arg(short, long)]
        query: String,

        #[arg(short, long, default_value_t = 5)]
        limit: usize,

        #[arg(short, long)]
        ticker: Option<String>,

        #[arg(short, long)]
        year: Option<u16>,

        #[arg(long)]
        quarter: Option<String>,

        #[arg(short, long, default_value_t = false)]
        verbose: bool,
    },
}

pub async fn handle(action: DebugAction, ctx: &AppContext) -> Result<()> {
    match action {
        DebugAction::SearchVectors {
            query,
            limit,
            ticker,
            year,
            quarter,
            verbose,
        } => {
            let db_config = DbConfig::from_env().into_diagnostic()?;
            let db = Db::connect(&db_config).await.into_diagnostic()?;
            let embedder = factory::build_embedder(ctx).await?;

            if verbose {
                println!("Generating embeddings for query: '{}'", query);
            }

            let response = embedder
                .embed(
                    "voyage-4-large",
                    vec![query.clone()],
                    InputType::Query,
                    true,
                )
                .await
                .into_diagnostic()?;

            let query_vector = &response.embeddings[0].vector;

            let filters = SearchFilters {
                ticker,
                year,
                quarter,
            };

            if verbose {
                println!(
                    "Executing MongoDB Atlas Vector Search (Limit: {})...",
                    limit
                );
                println!("Filters applied: {:?}", filters);
            }

            let searcher = build_searcher(&db);

            // Pass the filters struct into the search
            let results = searcher
                .search_earnings(query_vector, limit, filters)
                .await
                .into_diagnostic()?;

            let min_score_threshold = 0.70;

            let high_quality_results: Vec<_> = results
                .into_iter()
                .filter(|r| r.score >= min_score_threshold)
                .collect();

            if high_quality_results.is_empty() {
                println!("\nNo highly relevant chunks found (Scores were below threshold).");
                println!(
                    "This usually means the specific topic wasn't discussed in the filtered calls."
                );
            } else {
                for (i, result) in high_quality_results.iter().enumerate() {
                    println!("\n=== Result {} (Score: {:.4}) ===", i + 1, result.score);
                    println!(
                        "Ticker: {} | Year: {} | Quarter: {}",
                        result.ticker, result.year, result.quarter
                    );
                    println!("Chunk ID: {}", result.id.to_hex());
                    println!("Text:\n{}\n", result.text);
                }
            }
        }
    }
    Ok(())
}
