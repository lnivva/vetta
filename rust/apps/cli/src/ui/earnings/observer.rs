use indicatif::ProgressBar;
use vetta_core::domain::Transcript;
use vetta_core::earnings::{EarningsObserver, PipelineStage};

use crate::ui::{self, ARROW, Styles, WARN, error_msg, success_msg};

pub struct EarningsCliObserver {
    spinner: ProgressBar,
}

impl EarningsCliObserver {
    pub fn new() -> Self {
        let spinner = ui::spinner();
        Self { spinner }
    }
}

impl EarningsObserver for EarningsCliObserver {
    fn on_pipeline_started(&self, file_path: &str, replace: bool) {
        let mode = if replace { " (replace)" } else { "" };
        self.spinner
            .set_message(format!("Processing {file_path}{mode}"));
    }

    fn on_validating_file(&self, _file_path: &str) {
        self.spinner.set_message("Validating media file…");
    }

    fn on_validation_passed(&self, info: &str) {
        self.spinner
            .println(success_msg(&format!("Validated: {info}")));
    }

    fn on_checking_duplicate(&self, _file_path: &str) {
        self.spinner.set_message("Checking for duplicates…");
    }

    fn on_duplicate_overridden(&self, existing_call_id: &str) {
        self.spinner.println(format!(
            "{} {}",
            console::style(WARN).yellow(),
            console::style(format!(
                "Replacing existing call {ARROW} {existing_call_id}"
            ))
            .yellow()
        ));
    }

    fn on_duplicate_check_passed(&self) {
        self.spinner.println(success_msg("No duplicate found"));
    }

    fn on_transcription_started(&self) {
        self.spinner.set_message("Transcribing…");
    }

    fn on_transcription_progress(&self, segments_completed: u32) {
        self.spinner.set_message(format!(
            "Transcribing… {segments_completed} segments so far"
        ));
    }

    fn on_diarization_started(&self) {
        self.spinner.set_message("Identifying speakers…");
    }

    fn on_diarization_progress(&self, speakers_identified: u32) {
        self.spinner
            .set_message(format!("Identifying speakers… {speakers_identified} found"));
    }

    fn on_diarization_complete(&self, speaker_count: u32) {
        self.spinner.println(success_msg(&format!(
            "Diarization complete ({speaker_count} speakers)"
        )));
    }

    fn on_transcription_complete(&self, transcript: &Transcript) {
        self.spinner.println(success_msg(&format!(
            "Transcription complete ({} segments)",
            transcript.segments.len()
        )));
    }

    fn on_chunk_optimization_started(&self, raw_chunk_count: u32) {
        self.spinner
            .set_message(format!("Optimizing {raw_chunk_count} chunks…"));
    }

    fn on_chunk_optimization_progress(&self, chunks_processed: u32, total_chunks: u32) {
        self.spinner.set_message(format!(
            "Optimizing chunks… {chunks_processed}/{total_chunks}"
        ));
    }

    fn on_chunk_optimization_complete(&self, final_chunk_count: u32) {
        self.spinner.println(success_msg(&format!(
            "Chunk optimization complete ({final_chunk_count} chunks)"
        )));
    }

    fn on_storing_call(&self, chunk_count: u32) {
        self.spinner
            .set_message(format!("Storing call and {chunk_count} chunks…"));
    }

    fn on_call_stored(&self, call_id: &str, chunk_count: u32) {
        self.spinner.println(success_msg(&format!(
            "Stored {chunk_count} chunks {ARROW} {call_id}"
        )));
    }

    fn on_embedding_started(&self, chunk_count: u32) {
        self.spinner
            .set_message(format!("Generating embeddings for {chunk_count} chunks…"));
    }

    fn on_embedding_progress(&self, chunks_embedded: u32, total_chunks: u32) {
        self.spinner.set_message(format!(
            "Generating embeddings… {chunks_embedded}/{total_chunks}"
        ));
    }

    fn on_embedding_complete(&self, chunk_count: u32) {
        if chunk_count > 0 {
            self.spinner.println(success_msg(&format!(
                "Embeddings generated ({chunk_count} chunks)"
            )));
        } else {
            self.spinner.println(format!(
                "{} {}",
                console::style(WARN).yellow(),
                console::style("Embedding generation skipped (not yet implemented)").dim()
            ));
        }
    }

    fn on_storing_embeddings(&self, chunk_count: u32) {
        self.spinner
            .set_message(format!("Storing embeddings for {chunk_count} chunks…"));
    }

    fn on_embeddings_stored(&self, chunk_count: u32) {
        if chunk_count > 0 {
            self.spinner.println(success_msg(&format!(
                "Embeddings stored ({chunk_count} chunks)"
            )));
        } else {
            self.spinner.println(format!(
                "{} {}",
                console::style(WARN).yellow(),
                console::style("Embedding storage skipped (not yet implemented)").dim()
            ));
        }
    }

    fn on_pipeline_complete(
        &self,
        call_id: &str,
        chunk_count: u32,
        speaker_count: u32,
        duration_secs: f64,
    ) {
        self.spinner.finish_and_clear();

        let summary =
            format!("{chunk_count} chunks, {speaker_count} speakers, {duration_secs:.1}s");

        println!(
            "\n{}\n",
            success_msg(&format!(
                "Pipeline complete {ARROW} {} ({})",
                Styles::heading().apply_to(call_id),
                Styles::stat().apply_to(summary),
            ))
        );
    }

    fn on_pipeline_failed(&self, stage: PipelineStage, error_message: &str) {
        self.spinner.finish_and_clear();
        eprintln!(
            "\n{}\n",
            error_msg(&format!("Pipeline failed at {stage}: {error_message}"))
        );
    }
}
