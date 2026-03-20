use clap::{Subcommand, ValueEnum};
use miette::{IntoDiagnostic, Result};
use std::path::PathBuf;

use crate::{
    context::{AppContext, OutputMode},
    infra::factory,
    output,
    reporter::PipelineReporter,
};

use vetta_core::domain::Quarter as CoreQuarter;
use vetta_core::earnings_processor::{EarningsProcessor, ProcessRequest};

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

        #[arg(long)]
        out: Option<PathBuf>,

        #[arg(long)]
        print: bool,

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
        out,
        print,
        replace,
    } = action;

    let file = std::fs::canonicalize(&file).into_diagnostic()?;

    let stt = factory::build_stt(ctx).await?;
    let processor = EarningsProcessor::from_env(stt).await.into_diagnostic()?;

    let output_mode = match (print, out.is_some()) {
        (true, true) => OutputMode::Both,
        (true, false) => OutputMode::Pretty,
        (false, true) => OutputMode::Json,
        (false, false) => OutputMode::Pretty,
    };

    let reporter = PipelineReporter::new(ctx, matches!(output_mode, OutputMode::Pretty));

    let transcript = processor
        .process(
            ProcessRequest {
                file_path: file.to_string_lossy().into(),
                ticker,
                year,
                quarter: quarter.into(),
                language: Some("en".into()),
                initial_prompt: Some("Earnings call transcript".into()),
                replace,
            },
            |event| reporter.handle(&event),
        )
        .await
        .into_diagnostic()?;

    output::emit(ctx, &transcript, out.as_deref(), output_mode)?;

    Ok(())
}
