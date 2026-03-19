pub mod earnings;

use crate::{cli::Command, context::AppContext};
use miette::Result;

pub async fn dispatch(cmd: Command, ctx: &AppContext) -> Result<()> {
    match cmd {
        Command::Earnings { action } => earnings::handle(action, ctx).await,
    }
}
