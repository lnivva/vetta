use crate::context::AppContext;
use colored::*;
use vetta_core::earnings_processor::PipelineEvent;

pub struct PipelineReporter {
    quiet: bool,
}

impl PipelineReporter {
    pub fn new(ctx: &AppContext) -> Self {
        Self { quiet: ctx.quiet }
    }

    pub fn handle(&self, event: &PipelineEvent) {
        if self.quiet {
            return;
        }

        match event {
            PipelineEvent::ValidationPassed { .. } => {
                println!("{}", "✔ VALIDATION PASSED".green().bold());
            }
            PipelineEvent::TranscriptionProgress { segments } => {
                print!("\rTranscribing… {segments}");
            }
            PipelineEvent::TranscriptionComplete { transcript } => {
                println!(
                    "\n✔ Transcription complete ({} segments)",
                    transcript.segments.len()
                );
            }
            PipelineEvent::Stored { call_id, .. } => {
                println!("✔ Stored → {}", call_id);
            }
            _ => {}
        }
    }
}
