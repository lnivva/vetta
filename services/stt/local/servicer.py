"""
gRPC Servicer for the Whisper Speech-to-Text service.

This module coordinates the transcription flow:
1. resolve and preprocess audio.
2. optionally run diarization.
3. transcribe with Whisper.
4. post-process and apply speaker labels.
5. stream transcript chunks back to the caller.
"""

from __future__ import annotations

import logging
import threading
from concurrent.futures import ThreadPoolExecutor, Future
from typing import Any

import grpc
from faster_whisper import WhisperModel
from huggingface_hub import login

from audio import (
    AudioResolver,
    AudioPreprocessor,
    AudioValidationError,
    AudioFetchError,
    AudioDecodeError,
)
from postprocessor import TranscriptPostProcessor, PostProcessorConfig
from settings import Settings
from speech import speech_pb2_grpc, speech_pb2

logger = logging.getLogger(__name__)

_INFERENCE_ERRORS = (RuntimeError, ValueError, OSError)


def _load_diarization():
    """
    Lazily import diarization support.

    Returns:
        (DiarizationPipeline, DiarizationResult) or (None, None)
    """
    try:
        from diarization import DiarizationPipeline, DiarizationResult

        return DiarizationPipeline, DiarizationResult
    except Exception as e:
        logger.warning(
            "Diarization support is not available; continuing without it (%s)",
            str(e),
        )
        return None, None


class WhisperServicer(speech_pb2_grpc.SpeechToTextServicer):
    """
    gRPC service implementation for streaming speech-to-text responses.

    The servicer owns:
    - audio resolution and validation,
    - Whisper model inference,
    - optional speaker diarization,
    - transcript post-processing,
    - conversion of internal segments into protobuf messages.
    """

    def __init__(self, settings: Settings):
        """
        Initialize the service with runtime settings.

        Diarization is intentionally initialized lazily so the service can
        start even when diarization dependencies are missing (e.g. dev).
        """
        s = settings
        # --- Hugging Face Login ---
        if s.diarization.enabled and s.diarization.hf_token:
            try:
                login(token=s.diarization.hf_token)
                logger.info("Successfully authenticated with Hugging Face Hub")
            except Exception as e:
                logger.error(f"Hugging Face login failed: {e}")
        # --------------------------

        self.inference = s.inference

        # ── Audio ────────────────────────────────────
        self._resolver = AudioResolver(
            max_bytes=s.service.max_audio_size_mb * 1024 * 1024,
        )
        self._preprocessor = AudioPreprocessor()

        # ── Whisper ──────────────────────────────────
        self.model = WhisperModel(
            s.model.size,
            device=s.model.device,
            compute_type=s.model.compute_type,
            download_root=s.model.download_dir,
            num_workers=s.concurrency.num_workers,
            cpu_threads=s.concurrency.cpu_threads,
        )

        # ── Diarization ──────────────────────────────
        self._diarization_config = s.diarization
        self.diarizer = None
        self._diarization_lock = threading.Lock()

        # ── Execution ────────────────────────────────
        self._executor = ThreadPoolExecutor(max_workers=2)

        # ── Post-processing Config ───────────────────
        pp_cfg = settings.postprocessing
        self._postprocessor_config = (
            PostProcessorConfig(
                enable_punctuation=pp_cfg.punctuation,
                enable_entity_correction=pp_cfg.entity_correction,
                enable_truecasing=pp_cfg.truecasing,
            )
            if pp_cfg.enabled
            else None
        )

    @staticmethod
    def _get_num_speakers(options) -> int:
        """Return the requested speaker count from request options."""
        return options.num_speakers if options.HasField("num_speakers") else 0

    @staticmethod
    def _whisper_segment_to_dict(seg) -> dict[str, Any]:
        """Convert a single faster-whisper segment into a canonical dict."""
        word_list = [
            {
                "start": w.start,
                "end": w.end,
                "start_time": w.start,
                "end_time": w.end,
                "text": w.word,
                "confidence": w.probability,
                "speaker_id": "",
            }
            for w in (seg.words or [])
        ]

        return {
            "start": seg.start,
            "end": seg.end,
            "start_time": seg.start,
            "end_time": seg.end,
            "text": seg.text,
            "confidence": seg.avg_logprob,
            "speaker_id": "",
            "words": word_list,
        }

    @classmethod
    def _whisper_segments_to_dicts(cls, segments) -> list[dict[str, Any]]:
        """
        Consume the faster-whisper segment generator and convert each
        segment into a plain dictionary.
        """
        return [cls._whisper_segment_to_dict(seg) for seg in segments]

    @staticmethod
    def _seg_to_chunk(seg: dict[str, Any]) -> speech_pb2.TranscriptChunk:
        """Convert a normalised segment dict into a protobuf TranscriptChunk."""
        return speech_pb2.TranscriptChunk(
            start_time=seg["start_time"],
            end_time=seg["end_time"],
            text=seg.get("text", ""),
            speaker_id=seg.get("speaker_id", ""),
            confidence=seg.get("confidence", 0.0),
            words=[
                speech_pb2.Word(
                    start_time=w["start_time"],
                    end_time=w["end_time"],
                    text=w.get("text", ""),
                    confidence=w.get("confidence", 0.0),
                    speaker_id=w.get("speaker_id", ""),
                )
                for w in seg.get("words", [])
            ],
        )

    def Transcribe(self, request, context):
        """
        Process an audio request and stream transcript chunks to the client.

        This method orchestrates the complete speech-to-text pipeline, dynamically
        choosing between a low-latency streaming fast-path and a high-accuracy batch
        path based on the requested features (diarization and post-processing).

        Pipeline Stages:
            1. Audio Resolution: Fetches, validates, and decodes the requested audio.
            2. Parallel Execution (Optional): Kicks off speaker diarization in a background thread.
            3. Whisper Inference: Transcribes the audio using the faster-whisper model.
            4. Execution Routing:
                - Fast Path: If diarization and post-processing are disabled, segments
                  are yielded to the client immediately as Whisper generates them.
                - Batch Path: If diarization or post-processing is enabled, the generator
                  is exhausted, speaker labels are applied, and the text is refined
                  (stitched, truecased, punctuated) before yielding the final chunks.

        Args:
            request: The gRPC request object containing the audio payload/URI and
                transcription configuration options (e.g., language, diarization flag).
            context (grpc.ServicerContext): The gRPC context used for managing the RPC
                lifecycle, logging, and aborting on errors.

        Yields:
            speech_pb2.TranscriptChunk: Protobuf messages containing segment text,
                start/end times, confidence scores, and word-level timestamps.

        Raises:
            grpc.RpcError: If the streaming connection to the client is lost.

        Aborts (via context.abort):
            INVALID_ARGUMENT (400): If the audio is unreadable, exceeds size limits,
                or if diarization is requested by the client but disabled on the server.
            INTERNAL (500): If model inference fails, diarization crashes (when marked
                as strictly required), or an unexpected error occurs during streaming.
        """
        inf = self.inference
        prompt = request.options.initial_prompt or inf.initial_prompt or None

        # ── Request-scoped Postprocessor ──────────────
        postprocessor = (
            TranscriptPostProcessor(self._postprocessor_config)
            if self._postprocessor_config
            else None
        )

        # ── Resolve audio ─────────────────────────────
        try:
            audio, log_source, source_type = self._resolver.resolve(request)
        except (AudioValidationError, AudioFetchError) as exc:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, str(exc))
            return

        # ── Diarization flags & Lazy Initialization ───
        diarize = self._diarization_config.enabled

        if diarize:
            if self.diarizer is None:
                with self._diarization_lock:
                    if self.diarizer is None:
                        DiarizationPipeline, _ = _load_diarization()

                        if DiarizationPipeline is None:
                            if self._diarization_config.required:
                                context.abort(
                                    grpc.StatusCode.INTERNAL,
                                    "Diarization requested but dependencies are not installed.",
                                )

                            logger.warning(
                                "Diarization requested but unavailable; "
                                "continuing without speaker labels"
                            )
                            diarize = False
                        else:
                            try:
                                self.diarizer = DiarizationPipeline(
                                    self._diarization_config
                                )
                            except Exception:
                                logger.exception("Failed to initialize diarization")
                                if self._diarization_config.required:
                                    context.abort(
                                        grpc.StatusCode.INTERNAL,
                                        "Failed to initialize diarization",
                                    )
                                diarize = False

        # ── Preprocess ────────────────────────────────
        try:
            whisper_input, diar_input = self._preprocessor.prepare(
                audio,
                diarize=diarize,
            )
        except AudioDecodeError as exc:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, str(exc))
            return

        # ── Phase 1: Diarization (run in parallel with Whisper) ───
        diar_future: Future | None = None
        if diarize and diar_input is not None and self.diarizer is not None:
            num_speakers = self._get_num_speakers(request.options)

            def _run_diarizer_safely():
                # Prevent concurrent GPU inference collisions
                with self._diarization_lock:
                    return self.diarizer.run(
                        diar_input,
                        min_speakers=num_speakers,
                        max_speakers=num_speakers,
                    )

            diar_future = self._executor.submit(_run_diarizer_safely)

        # ── Phase 2: Whisper ──────────────────────────
        try:
            segments_iter, info = self.model.transcribe(
                whisper_input,
                language=request.language or None,
                beam_size=inf.beam_size,
                vad_filter=inf.vad_filter,
                vad_parameters={
                    "min_silence_duration_ms": inf.vad_min_silence_ms,
                },
                word_timestamps=inf.word_timestamps,
                initial_prompt=prompt,
                no_speech_threshold=inf.no_speech_threshold,
                log_prob_threshold=inf.log_prob_threshold,
                compression_ratio_threshold=inf.compression_ratio_threshold,
            )
        except _INFERENCE_ERRORS:
            logger.exception("Whisper failed")
            # Cancel pending diarization if whisper failed
            if diar_future is not None:
                diar_future.cancel()
            context.abort(grpc.StatusCode.INTERNAL, "Transcription failed")
            return

        # ── Fast Path vs Batch Path ───────────────────
        requires_batching = diarize or (postprocessor is not None)

        if not requires_batching:
            # ── True Streaming Fast-Path ──────────────
            try:
                for seg in segments_iter:
                    seg_dict = self._whisper_segment_to_dict(seg)
                    yield self._seg_to_chunk(seg_dict)
                return
            except grpc.RpcError:
                raise
            except _INFERENCE_ERRORS:
                logger.exception("Streaming failed")
                context.abort(grpc.StatusCode.INTERNAL, "Streaming failed")

        # ── Phase 3: Collect for Batch Processing ─────
        try:
            seg_dicts = self._whisper_segments_to_dicts(segments_iter)
        except _INFERENCE_ERRORS:
            logger.exception("Whisper iteration failed")
            if diar_future is not None:
                diar_future.cancel()
            context.abort(grpc.StatusCode.INTERNAL, "Transcription failed")

        # ── Phase 4: Resolve diarization future ───────
        diarization = None
        if diar_future is not None:
            try:
                diarization = diar_future.result()
            except Exception:
                logger.exception("Diarization failed")
                if self._diarization_config.required:
                    context.abort(
                        grpc.StatusCode.INTERNAL,
                        "Diarization failed",
                    )
                diarization = None

        # ── Phase 5: Apply diarization labels ─────────
        if diarization is not None:
            diarization.assign_speakers(seg_dicts)

        # ── Phase 6: Post-processing ──────────────────
        if postprocessor:
            seg_dicts = postprocessor.process_segments(
                seg_dicts,
                preserve_raw=True,
                stitch=True,
            )

        # ── Phase 7: Stream batched results ───────────
        try:
            for seg in seg_dicts:
                yield self._seg_to_chunk(seg)
        except grpc.RpcError:
            raise
        except _INFERENCE_ERRORS:
            logger.exception("Streaming failed")
            context.abort(grpc.StatusCode.INTERNAL, "Streaming failed")
