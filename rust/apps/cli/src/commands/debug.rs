use clap::{Args, Subcommand};
use miette::{IntoDiagnostic, Result};
use std::io::Write;

use crate::{
    cli::{CliOutputFormat, PayloadDriven},
    context::AppContext,
    infra::factory,
    ui::{INDENT, Styles, info_msg, separator},
};

use crate::ui::get_writer;
use vetta_core::db::{Db, DbConfig};
use vetta_core::{InputType, SearchFilters, VectorSearchResult, build_searcher};

#[derive(Args, Debug, Clone, serde::Deserialize)]
pub struct SearchArgs {
    #[arg(short, long)]
    pub query: Option<String>,
    #[arg(short, long, default_value = "5")]
    pub limit: Option<usize>,
    #[arg(short, long)]
    pub ticker: Option<String>,
    #[arg(short, long)]
    pub year: Option<u16>,
    #[arg(long)]
    pub quarter: Option<String>,
    /// Emit intermediary status logs to stderr
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum DebugAction {
    SearchVectors(SearchArgs),
}

#[derive(Debug, serde::Deserialize)]
pub struct SearchPayload {
    pub query: String,
    pub limit: usize,
    pub ticker: Option<String>,
    pub year: Option<u16>,
    pub quarter: Option<String>,
}

impl PayloadDriven for SearchPayload {
    type CliArgs = SearchArgs;

    fn from_cli(args: &Self::CliArgs) -> Option<Self> {
        args.query.as_ref().map(|q| Self {
            query: q.clone(),
            limit: args.limit.unwrap_or(5),
            ticker: args.ticker.clone(),
            year: args.year,
            quarter: args.quarter.clone(),
        })
    }

    fn merge_cli(&mut self, args: &Self::CliArgs) {
        if let Some(q) = &args.query {
            self.query = q.clone();
        }
        if let Some(l) = args.limit {
            self.limit = l;
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
    }
}

pub async fn handle(action: DebugAction, ctx: &AppContext) -> Result<()> {
    let DebugAction::SearchVectors(args) = action;
    let payload = SearchPayload::resolve(ctx, &args)?;

    // 1. Setup Infrastructure
    let db_config = DbConfig::from_env().into_diagnostic()?;
    let db = Db::connect(&db_config).await.into_diagnostic()?;
    let embedder = factory::build_embedder(ctx).await?;
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
            vec![payload.query],
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

    // 3. Search (Logs to stderr)
    if args.verbose {
        eprintln!(
            "{}",
            info_msg(&format!(
                "Searching MongoDB Atlas (Limit: {})...",
                payload.limit
            ))
        );
    }

    let results = searcher
        .search_earnings(query_vector, payload.limit, filters)
        .await
        .into_diagnostic()?;

    // 4. Resolve Output Destination (Dynamic Write)
    let mut writer = get_writer(&ctx.output)?;

    // 5. Render to format
    match ctx.format {
        CliOutputFormat::Json => {
            let json = serde_json::to_string_pretty(&results).into_diagnostic()?;
            writeln!(writer, "{}", json).into_diagnostic()?;
        }
        CliOutputFormat::Plain => {
            render_plain_results(results, &mut writer)?;
        }
    }

    Ok(())
}

fn render_plain_results(results: Vec<VectorSearchResult>, out: &mut dyn Write) -> Result<()> {
    if results.is_empty() {
        writeln!(out, "\n{INDENT}No relevant segments found.").into_diagnostic()?;
        return Ok(());
    }

    for (i, res) in results.iter().enumerate() {
        writeln!(out, "\n{INDENT}{}", separator()).into_diagnostic()?;
        writeln!(
            out,
            "{INDENT}{} (Score: {})",
            Styles::heading().apply_to(format!("Result #{}", i + 1)),
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
