use clap::{Args, Subcommand};
use miette::{IntoDiagnostic, Result, WrapErr, bail};
use std::io::Write;
use std::process::Command;

use crate::{
    cli::{CliOutputFormat, PayloadDriven},
    context::AppContext,
    infra::factory,
    ui::{INDENT, Styles, get_writer, info_msg, separator, success_msg},
};

use vetta_core::db::{Db, DbConfig};
use vetta_core::{InputType, SearchFilters, VectorSearchResult, build_searcher};

#[derive(Args, Debug, Clone, serde::Deserialize)]
pub struct SearchArgs {
    #[arg(short, long)]
    pub query: Option<String>,
    #[arg(short, long, default_value = "5")]
    pub limit: Option<usize>,
    #[arg(short, long, default_value = "25")]
    pub candidate_pool: Option<usize>,
    #[arg(short, long)]
    pub ticker: Option<String>,
    #[arg(short, long)]
    pub year: Option<u16>,
    #[arg(long)]
    pub quarter: Option<String>,
    /// Minimum relevance score required to keep a reranked result
    #[arg(short = 'm', long, default_value = "0.55")]
    pub min_score: f64,
    /// Emit intermediary status logs to stderr
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum DebugAction {
    /// Search vectors in the database
    SearchVectors(SearchArgs),

    /// Explicitly trigger the database migration/index check
    MigrateDb,
}

#[derive(Debug, serde::Deserialize)]
pub struct SearchPayload {
    pub query: String,
    pub limit: usize,
    pub candidate_pool: usize,
    pub ticker: Option<String>,
    pub year: Option<u16>,
    pub quarter: Option<String>,
    pub min_score: f64,
}

impl PayloadDriven for SearchPayload {
    type CliArgs = SearchArgs;

    fn from_cli(args: &Self::CliArgs) -> Option<Self> {
        args.query.as_ref().map(|q| Self {
            query: q.clone(),
            limit: args.limit.unwrap_or(5),
            candidate_pool: args.candidate_pool.unwrap_or(25),
            ticker: args.ticker.clone(),
            year: args.year,
            quarter: args.quarter.clone(),
            min_score: args.min_score,
        })
    }

    fn merge_cli(&mut self, args: &Self::CliArgs) {
        if let Some(q) = &args.query {
            self.query = q.clone();
        }
        if let Some(l) = args.limit {
            self.limit = l;
        }
        if let Some(c) = args.candidate_pool {
            self.candidate_pool = c;
        }
        if let Some(t) = &args.ticker {
            self.ticker = Some(t.clone());
        }
        if let Some(y) = args.year {
            self.year = Some(y);
        }
        if let Some(q) = &args.quarter {
            self.quarter = Some(q.clone());
        }
        self.min_score = args.min_score;
    }
}

pub async fn handle(action: DebugAction, ctx: &AppContext) -> Result<()> {
    match action {
        DebugAction::SearchVectors(args) => handle_search_vectors(args, ctx).await,
        DebugAction::MigrateDb => handle_migrate_db(ctx),
    }
}

async fn handle_search_vectors(args: SearchArgs, ctx: &AppContext) -> Result<()> {
    let payload = SearchPayload::resolve(ctx, &args)?;

    // 1. Setup Infrastructure
    let db_config = DbConfig::new(
        ctx.config.mongodb_uri.clone(),
        ctx.config.mongodb_database.clone(),
    );

    let db = Db::connect(&db_config).await.into_diagnostic()?;
    let embedder = factory::build_embedder(ctx).await?;
    let reranker = factory::build_reranker(ctx).await?;
    let searcher = build_searcher(&db);

    // 2. Embed Query (Logs to stderr)
    if args.verbose {
        eprintln!(
            "{}",
            info_msg(&format!("Embedding query: '{}'", payload.query))
        );
    }

    let response = embedder
        .embed(
            "voyage-4-large",
            vec![payload.query.clone()],
            InputType::Query,
            true,
        )
        .await
        .into_diagnostic()?;

    let query_vector = &response.embeddings[0].vector;
    let filters = SearchFilters {
        ticker: payload.ticker,
        year: payload.year,
        quarter: payload.quarter,
    };

    // 3. Stage 1: Vector Search (Fetch Candidate Pool)
    if args.verbose {
        eprintln!(
            "{}",
            info_msg(&format!(
                "Searching MongoDB Atlas (Fetching {} candidates)...",
                payload.candidate_pool
            ))
        );
    }

    let candidate_results = searcher
        .search_earnings(query_vector, payload.candidate_pool, filters)
        .await
        .into_diagnostic()?;

    // 4. Stage 2: Rerank Candidates
    let final_results = if candidate_results.is_empty() {
        vec![]
    } else {
        if args.verbose {
            eprintln!(
                "{}",
                info_msg(&format!(
                    "Reranking {} candidates with voyage-rerank-2.5 (Target Top K: {}, Min Score: {})...",
                    candidate_results.len(),
                    payload.limit,
                    payload.min_score
                ))
            );
        }

        // Extract raw document text to send to Voyage AI
        let documents: Vec<String> = candidate_results.iter().map(|r| r.text.clone()).collect();

        let top_k = i32::try_from(payload.limit)
            .into_diagnostic()
            .wrap_err("--limit exceeds the reranker protocol range (i32)")?;

        // Call the reranker via UDS using the upgraded model
        let rerank_response = reranker
            .rerank("rerank-2.5", &payload.query, documents, Some(top_k))
            .await
            .into_diagnostic()?;

        // Map the relevance scores back to our original search results AND filter
        let mut reranked = Vec::with_capacity(rerank_response.results.len());
        for res in rerank_response.results {
            let score = res.relevance_score as f64;

            // Apply the cutoff threshold
            if score >= payload.min_score {
                if let Some(mut original) = candidate_results.get(res.index).cloned() {
                    original.score = score;
                    reranked.push(original);
                }
            } else if args.verbose {
                eprintln!(
                    "{}",
                    Styles::dimmed().apply_to(format!(
                        "Dropped result below threshold (Score: {:.4})",
                        score
                    ))
                );
            }
        }

        reranked
    };

    // 5. Resolve Output Destination (Dynamic Write)
    let mut writer = get_writer(&ctx.output)?;

    // 6. Render to format
    match ctx.format {
        CliOutputFormat::Json => {
            let debug_payload = serde_json::json!({
                "candidates": candidate_results,
                "final": final_results,
            });
            let json = serde_json::to_string_pretty(&debug_payload).into_diagnostic()?;
            writeln!(writer, "{}", json).into_diagnostic()?;
        }
        CliOutputFormat::Plain => {
            writeln!(
                writer,
                "\n{}",
                info_msg("--- Stage 1: Vector Search Candidates ---")
            )
            .into_diagnostic()?;
            render_plain_results(&candidate_results, "Vector Score", &mut writer)?;

            writeln!(
                writer,
                "\n{}",
                info_msg("--- Stage 2: Reranked Results (Filtered) ---")
            )
            .into_diagnostic()?;
            render_plain_results(&final_results, "Relevance Score", &mut writer)?;
        }
    }

    Ok(())
}

fn handle_migrate_db(ctx: &AppContext) -> Result<()> {
    if ctx.debug {
        tracing::debug!("Ensuring database indexes are up to date...");
    }

    let mut migrate_bin = std::env::current_exe()
        .into_diagnostic()
        .wrap_err("Failed to resolve current executable path")?;
    migrate_bin.pop();
    migrate_bin.push("vetta_migrate");

    let mut cmd = Command::new(&migrate_bin);

    cmd.env("MONGODB_URI", &ctx.config.mongodb_uri);
    cmd.env("MONGODB_DATABASE", &ctx.config.mongodb_database);

    if !ctx.debug {
        cmd.env("RUST_LOG", "error");
    }

    let status = cmd
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .status()
        .into_diagnostic()
        .wrap_err("Failed to execute vetta_migrate binary. Did you build it with: `cargo build --bin vetta_migrate`?")?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        bail!("Database migration failed with exit code: {}", code);
    }

    eprintln!(
        "{}",
        success_msg("Database migration check passed successfully.")
    );

    Ok(())
}

fn render_plain_results(
    results: &[VectorSearchResult],
    score_label: &str,
    out: &mut dyn Write,
) -> Result<()> {
    if results.is_empty() {
        writeln!(out, "\n{INDENT}No relevant segments found.").into_diagnostic()?;
        return Ok(());
    }

    for (i, res) in results.iter().enumerate() {
        writeln!(out, "\n{INDENT}{}", separator()).into_diagnostic()?;
        writeln!(
            out,
            "{INDENT}{} ({}: {})",
            Styles::heading().apply_to(format!("Result #{}", i + 1)),
            score_label,
            Styles::stat().apply_to(format!("{:.4}", res.score))
        )
        .into_diagnostic()?;

        writeln!(
            out,
            "{INDENT}{} {} | {} | {}",
            Styles::dimmed().apply_to("Source:"),
            res.ticker,
            res.year,
            res.quarter
        )
        .into_diagnostic()?;

        writeln!(out, "{INDENT}{}\n", separator()).into_diagnostic()?;

        for line in textwrap::fill(res.text.trim(), 80).lines() {
            writeln!(out, "{INDENT}{line}").into_diagnostic()?;
        }
        writeln!(out).into_diagnostic()?;
    }

    out.flush().into_diagnostic()?;
    Ok(())
}
