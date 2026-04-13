use clap::{Subcommand, ValueEnum};
use miette::{IntoDiagnostic, Result};
use std::path::PathBuf;

use crate::{
    cli::CliOutputOptions,
    context::AppContext,
    infra::factory,
    ui::earnings::{EarningsCliObserver, print_transcript},
};

use vetta_core::db::{Db, DbConfig};
use vetta_core::earnings::{EarningsProcessor, ProcessEarningsCallRequest};
use vetta_core::stt::domain::Quarter as CoreQuarter;

#[derive(Debug, Clone, ValueEnum)]
pub enum CliQuarter {
    Q1,
    Q2,
    Q3,
    Q4,
}

impl From<CliQuarter> for CoreQuarter {
    fn from(q: CliQuarter) -> Self {
        match q {
            CliQuarter::Q1 => CoreQuarter::Q1,
            CliQuarter::Q2 => CoreQuarter::Q2,
            CliQuarter::Q3 => CoreQuarter::Q3,
            CliQuarter::Q4 => CoreQuarter::Q4,
        }
    }
}

#[derive(Subcommand)]
pub enum EarningsAction {
    Process {
        #[arg(short, long)]
        file: PathBuf,

        #[arg(short, long)]
        ticker: String,

        #[arg(short, long)]
        year: u16,

        #[arg(short, long, value_enum)]
        quarter: CliQuarter,

        #[arg(long, default_value = "false")]
        replace: bool,
    },
}

pub async fn handle(action: EarningsAction, ctx: &AppContext) -> Result<()> {
    let EarningsAction::Process {
        file,
        ticker,
        year,
        quarter,
        replace,
    } = action;

    let file = std::fs::canonicalize(&file).into_diagnostic()?;

    let db_config = DbConfig::from_env().into_diagnostic()?;
    let db = Db::connect(&db_config).await.into_diagnostic()?;

    let stt = factory::build_stt(ctx).await?;
    let embedder = factory::build_embedder(ctx).await?;

    let processor = EarningsProcessor::new(stt, embedder, db);

    let observer = EarningsCliObserver::new(ctx.output, ctx.verbose);

    let request = ProcessEarningsCallRequest {
        file_path: file.to_string_lossy().into(),
        ticker,
        year,
        quarter: quarter.into(),
        language: Some("en".into()),
        initial_prompt: Some("Earnings call transcript".into()),
        replace,
    };

    let transcript = processor
        .process(request, &observer)
        .await
        .into_diagnostic()?;

    match ctx.output {
        CliOutputOptions::Json => {
            let json_out = serde_json::to_string_pretty(&transcript).into_diagnostic()?;
            println!("{json_out}");
        }
        CliOutputOptions::Plain => {
            print_transcript(&transcript)?;
        }
    }

    Ok(())
}
