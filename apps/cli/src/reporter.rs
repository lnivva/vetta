use colored::*;
use std::io::{self, Write};

use crate::context::AppContext;
use vetta_core::earnings_processor::PipelineEvent;

pub struct PipelineReporter {
    quiet: bool,
}

impl PipelineReporter {
    pub fn new(ctx: &AppContext, force_quiet: bool) -> Self {
        Self {
            quiet: ctx.quiet || force_quiet,
        }
    }

    pub fn handle(&self, event: &PipelineEvent) {
        if self.quiet {
            return;
        }

        match event {
            PipelineEvent::ValidationPassed { format_info } => {
                println!("{}", "✔ VALIDATION PASSED".green().bold());
                println!("   {:<10} {}", "Format:".dimmed(), format_info);
                println!();
                println!("{}", "Processing Pipeline:".bold().blue());
                println!("   1. [✔] Validation");
                println!("   2. [{}] Transcription", "RUNNING".yellow());
            }

            PipelineEvent::TranscriptionProgress { segments } => {
                print!("\r\x1B[K   Transcribing… {segments} segments");
                let _ = io::stdout().flush();
            }

            PipelineEvent::TranscriptionComplete { transcript } => {
                let seg_count = transcript.segments.len();
                let speaker_count = transcript.unique_speakers().len();

                println!(
                    "\r\x1B[K   2. [✔] Transcription ({} segments, {} speakers)",
                    seg_count, speaker_count
                );
            }

            PipelineEvent::StoringChunks { chunk_count } => {
                print!(
                    "   3. [{}] Storing {} chunks…",
                    "RUNNING".yellow(),
                    chunk_count
                );
                let _ = io::stdout().flush();
            }

            PipelineEvent::Stored { call_id, chunk_count } => {
                println!(
                    "\r\x1B[K   3. [✔] Stored ({} chunks → {})",
                    chunk_count, call_id
                );
            }
        }
    }
}