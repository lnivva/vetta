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
from concurrent.futures import ThreadPoolExecutor, Future
from typing import Any

import grpc
from faster_whisper import WhisperModel

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
        self.diarizer = None  # Lazy-initialized

        # ── Execution ────────────────────────────────
        self._executor = ThreadPoolExecutor(max_workers=2)

        # ── Post-processing ──────────────────────────
        pp_cfg = settings.postprocessing
        self._postprocessor = (
            TranscriptPostProcessor(
                PostProcessorConfig(
                    enable_punctuation=pp_cfg.punctuation,
                    enable_entity_correction=pp_cfg.entity_correction,
                    enable_truecasing=pp_cfg.truecasing,
                )
            )
            if pp_cfg.enabled
            else None
        )

    @staticmethod
    def _get_num_speakers(options) -> int:
        """Return the requested speaker count from request options."""
        return options.num_speakers if options.HasField("num_speakers") else 0

        # ------------------------------------------------------------------

    # Helpers: segment normalisation
    # ------------------------------------------------------------------

    @staticmethod
    def _whisper_segments_to_dicts(segments) -> list[dict[str, Any]]:
        """
        Consume the faster-whisper segment generator and convert each
        segment into a plain dictionary that is compatible with both
        the post-processor and the diarization result helpers.

        Keys use the servicer's canonical names (`start_time`, `end_time`,
        `speaker_id`) **and** the short aliases (`start`, `end`) so that
        both `DiarizationResult.assign_speakers` and
        `TranscriptPostProcessor.process_segments` work without adaptation.
        """
        results: list[dict[str, Any]] = []
        for seg in segments:
            word_list: list[dict[str, Any]] = []
            for w in seg.words or []:
                word_list.append(
                    {
                        "start": w.start,
                        "end": w.end,
                        "start_time": w.start,
                        "end_time": w.end,
                        "text": w.word,
                        "confidence": w.probability,
                        "speaker_id": "",
                    }
                )

            results.append(
                {
                    "start": seg.start,
                    "end": seg.end,
                    "start_time": seg.start,
                    "end_time": seg.end,
                    "text": seg.text,
                    "confidence": seg.avg_logprob,
                    "speaker_id": "",
                    "words": word_list,
                }
            )
        return results

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

        # ----------------------------------------------------------------------

    def Transcribe(self, request, context):
        """
        Stream a transcription response for the provided audio request.

        The full pipeline is:
        1. Resolve & validate audio input.
        2. Optionally run diarization (parallel-capable via executor).
        3. Run Whisper transcription.
        4. Collect all segments into dicts.
        5. Apply diarization speaker labels (segment + word level).
        6. Run full post-processing pipeline (stitching, entity
           correction, punctuation, truecasing).
        7. Yield protobuf chunks to the caller.
        """
        inf = self.inference
        prompt = request.options.initial_prompt or inf.initial_prompt or None

        # ── Resolve audio ─────────────────────────────
        try:
            audio, log_source, source_type = self._resolver.resolve(request)
        except (AudioValidationError, AudioFetchError) as exc:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, str(exc))
            return

            # ── Diarization flags ─────────────────────────
        diarize = request.options.diarization

        if diarize:
            if not self._diarization_config.enabled:
                context.abort(
                    grpc.StatusCode.INVALID_ARGUMENT,
                    "Diarization requested but not enabled in service configuration.",
                )
                return

            if self.diarizer is None:
                DiarizationPipeline, _ = _load_diarization()

                if DiarizationPipeline is None:
                    if self._diarization_config.required:
                        context.abort(
                            grpc.StatusCode.INTERNAL,
                            "Diarization requested but dependencies are not installed.",
                        )
                        return

                    logger.warning(
                        "Diarization requested but unavailable; "
                        "continuing without speaker labels"
                    )
                    diarize = False
                else:
                    try:
                        self.diarizer = DiarizationPipeline(self._diarization_config)
                    except Exception:
                        logger.exception("Failed to initialize diarization")
                        if self._diarization_config.required:
                            context.abort(
                                grpc.StatusCode.INTERNAL,
                                "Failed to initialize diarization",
                            )
                            return
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
            diar_future = self._executor.submit(
                self.diarizer.run,
                diar_input,
                min_speakers=num_speakers,
                max_speakers=num_speakers,
            )

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

            # ── Collect all Whisper segments ──────────────
        try:
            seg_dicts = self._whisper_segments_to_dicts(segments_iter)
        except _INFERENCE_ERRORS:
            logger.exception("Whisper iteration failed")
            if diar_future is not None:
                diar_future.cancel()
            context.abort(grpc.StatusCode.INTERNAL, "Transcription failed")
            return

            # ── Phase 3: Resolve diarization future ──────
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
                    return
                diarization = None

                # ── Phase 4: Apply diarization labels ────────
        if diarization is not None:
            diarization.assign_speakers(seg_dicts)

            # ── Phase 5: Post-processing ─────────────────
        if self._postprocessor:
            seg_dicts = self._postprocessor.process_segments(
                seg_dicts,
                preserve_raw=True,
                stitch=True,
            )

            # ── Phase 6: Stream results ──────────────────
        try:
            for seg in seg_dicts:
                yield self._seg_to_chunk(seg)
        except grpc.RpcError:
            raise
        except _INFERENCE_ERRORS:
            logger.exception("Streaming failed")
            context.abort(grpc.StatusCode.INTERNAL, "Streaming failed")
