use clap::{Args, Subcommand};
use miette::{IntoDiagnostic, Result};
use std::io::Write;
use std::path::PathBuf;

use crate::{
    cli::{CliOutputFormat, PayloadDriven},
    context::AppContext,
    infra::factory,
    ui::earnings::{EarningsCliObserver, print_transcript},
};

use crate::ui::get_writer;
use vetta_core::Quarter;
use vetta_core::db::{Db, DbConfig};
use vetta_core::earnings::{EarningsProcessor, ProcessEarningsCallRequest};

#[derive(Args, Debug, Clone)]
pub struct ProcessArgs {
    #[arg(short, long)]
    pub file: Option<PathBuf>,
    #[arg(short, long)]
    pub ticker: Option<String>,
    #[arg(short, long)]
    pub year: Option<u16>,
    #[arg(short, long)]
    pub quarter: Option<Quarter>,
    #[arg(long)]
    pub replace: bool,
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum EarningsAction {
    Process(ProcessArgs),
}

#[derive(Debug, serde::Deserialize)]
pub struct ProcessPayload {
    pub file: PathBuf,
    pub ticker: String,
    pub year: u16,
    pub quarter: Quarter,
    #[serde(default)]
    pub replace: bool,
}

impl PayloadDriven for ProcessPayload {
    type CliArgs = ProcessArgs;

    fn from_cli(args: &Self::CliArgs) -> Option<Self> {
        if let (Some(f), Some(t), Some(y), Some(q)) =
            (&args.file, &args.ticker, &args.year, &args.quarter)
        {
            Some(Self {
                file: f.clone(),
                ticker: t.clone(),
                year: *y,
                quarter: *q,
                replace: args.replace,
            })
        } else {
            None
        }
    }

    fn merge_cli(&mut self, args: &Self::CliArgs) {
        if let Some(f) = &args.file {
            self.file = f.clone();
        }
        if let Some(t) = &args.ticker {
            self.ticker = t.clone();
        }
        if let Some(y) = args.year {
            self.year = y;
        }
        if let Some(q) = &args.quarter {
            self.quarter = *q;
        }
        if args.replace {
            self.replace = true;
        }
    }
}

pub async fn handle(action: EarningsAction, ctx: &AppContext) -> Result<()> {
    let EarningsAction::Process(args) = action;

    let payload = ProcessPayload::resolve(ctx, &args)?;

    let file_path = std::fs::canonicalize(&payload.file).into_diagnostic()?;

    let db_config = DbConfig {
        uri: ctx.config.mongodb_uri.clone(),
        database: ctx.config.mongodb_database.clone(),
    };
    let db = Db::connect(&db_config).await.into_diagnostic()?;

    let stt = factory::build_stt(ctx).await?;
    let embedder = factory::build_embedder(ctx).await?;

    let processor = EarningsProcessor::new(stt, embedder, db);

    let observer = EarningsCliObserver::new(ctx.format, args.verbose);

    let request = ProcessEarningsCallRequest {
        file_path: file_path.to_string_lossy().into(),
        ticker: payload.ticker,
        year: payload.year,
        quarter: payload.quarter,
        language: Some("en".into()),
        initial_prompt: Some("Earnings call transcript".into()),
        replace: payload.replace,
    };

    let transcript = processor
        .process(request, &observer)
        .await
        .into_diagnostic()?;

    let mut writer = get_writer(&ctx.output)?;

    match ctx.format {
        CliOutputFormat::Json => {
            let json_out = serde_json::to_string_pretty(&transcript).into_diagnostic()?;
            writer.write_all(json_out.as_bytes()).into_diagnostic()?;
            writer.write_all(b"\n").into_diagnostic()?;
        }
        CliOutputFormat::Plain => {
            print_transcript(&transcript, &mut writer)?;
        }
    }

    Ok(())
}
