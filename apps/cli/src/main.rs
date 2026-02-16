use clap::{Parser, Subcommand, ValueEnum};
use vetta_core::domain::Quarter as CoreQuarter;

#[derive(Debug, Clone, ValueEnum)]
enum CliQuarter {
    Q1,
    Q2,
    Q3,
    Q4,
}

impl From<CliQuarter> for CoreQuarter {
    fn from(cli: CliQuarter) -> Self {
        match cli {
            CliQuarter::Q1 => CoreQuarter::Q1,
            CliQuarter::Q2 => CoreQuarter::Q2,
            CliQuarter::Q3 => CoreQuarter::Q3,
            CliQuarter::Q4 => CoreQuarter::Q4,
        }
    }
}

#[derive(Parser)]
#[command(name = "vetta")]
struct Cli {
    #[command(subcommand)]
    command: Resource,
}

#[derive(Subcommand)]
enum Resource {
    Earnings {
        #[command(subcommand)]
        action: EarningsAction,
    },
}

#[derive(Subcommand)]
enum EarningsAction {
    Process {
        #[arg(short, long)]
        file: String,

        #[arg(short, long)]
        ticker: String,

        #[arg(short, long)]
        year: u16,

        #[arg(short, long, value_enum)]
        quarter: CliQuarter,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Resource::Earnings { action } => match action {
            EarningsAction::Process {
                file: _,
                ticker,
                year,
                quarter,
            } => {
                let core_quarter: CoreQuarter = quarter.into();

                println!("Processing: {} {:?} {}", ticker, core_quarter, year);
            }
        },
    }
}
