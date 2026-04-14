mod config;
mod debug;
mod earnings;

pub use config::ConfigAction;
pub use debug::DebugAction;
pub use earnings::EarningsAction;

use crate::{cli::Command, context::AppContext};
use miette::Result;

pub async fn dispatch(cmd: Command, ctx: &AppContext) -> Result<()> {
    match cmd {
        Command::Earnings { action } => earnings::handle(action, ctx).await,
        Command::Debug { action } => debug::handle(action, ctx).await,
        Command::Config { action } => config::handle(action, ctx).await,
    }
}
