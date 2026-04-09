use indicatif::ProgressBar;
use vetta_core::domain::Transcript;
use vetta_core::earnings::EarningsObserver;

use crate::ui::{self, ARROW, success_msg};

pub struct EarningsCliObserver {
    spinner: ProgressBar,
}

impl EarningsCliObserver {
    pub fn new() -> Self {
        let spinner = ui::spinner();
        spinner.set_message("Validating…");
        Self { spinner }
    }
}

impl EarningsObserver for EarningsCliObserver {
    fn on_validation_passed(&self, info: &str) {
        self.spinner
            .println(success_msg(&format!("Validated: {info}")));
        self.spinner.set_message("Transcribing…");
    }

    fn on_transcription_progress(&self, segments: u32) {
        self.spinner
            .set_message(format!("Transcribing… {segments} segments so far"));
    }

    fn on_transcription_complete(&self, transcript: &Transcript) {
        self.spinner.println(success_msg(&format!(
            "Transcription complete ({} segments)",
            transcript.segments.len()
        )));
        self.spinner.set_message("Storing chunks…");
    }

    fn on_storing_chunks(&self, count: u32) {
        self.spinner.set_message(format!("Storing {count} chunks…"));
    }

    fn on_stored(&self, call_id: &str, count: u32) {
        self.spinner.println(success_msg(&format!(
            "Stored {count} chunks {ARROW} {call_id}"
        )));
        self.spinner.finish_and_clear();
    }
}
