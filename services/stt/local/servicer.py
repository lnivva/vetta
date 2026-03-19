"""
gRPC Servicer for the Whisper Speech-to-Text service.
"""

import logging
from concurrent.futures import ThreadPoolExecutor

import grpc
from faster_whisper import WhisperModel

from audio import (
    AudioResolver,
    AudioPreprocessor,
    AudioValidationError,
    AudioFetchError,
    AudioDecodeError,
)
from diarization import DiarizationPipeline, DiarizationResult
from settings import Settings
from speech import speech_pb2_grpc, speech_pb2

logger = logging.getLogger(__name__)

_INFERENCE_ERRORS = (RuntimeError, ValueError, OSError)


class WhisperServicer(speech_pb2_grpc.SpeechToTextServicer):
    """
    gRPC servicer for handling Speech-to-Text operations with optional
    speaker diarization.

    When diarization is enabled, the pipeline runs in two phases:
      1. Diarization runs first on the full audio (must see everything).
      2. Whisper streams segments, each tagged with a speaker label via
         temporal overlap against the cached diarization result.

    This avoids buffering all segments before yielding, so the client
    receives chunks as they are produced by Whisper.
    """

    def __init__(self, settings: Settings):
        s = settings
        self.inference = s.inference
        self._resolver = AudioResolver(
            max_bytes=s.service.max_audio_size_mb * 1024 * 1024,
        )
        self._preprocessor = AudioPreprocessor()

        self.model = WhisperModel(
            s.model.size,
            device=s.model.device,
            compute_type=s.model.compute_type,
            download_root=s.model.download_dir,
            num_workers=s.concurrency.num_workers,
            cpu_threads=s.concurrency.cpu_threads,
        )

        self.diarizer: DiarizationPipeline | None = None
        if s.diarization.enabled:
            self.diarizer = DiarizationPipeline(s.diarization)

        self._executor = ThreadPoolExecutor(max_workers=2)

    # ── Helpers ───────────────────────────────────────────────

    @staticmethod
    def _get_num_speakers(options) -> int:
        if options.HasField("num_speakers"):
            return options.num_speakers
        return 0

    @staticmethod
    def _segment_to_chunk(segment, speaker_id: str = "") -> speech_pb2.TranscriptChunk:
        """Convert a faster-whisper segment to a gRPC TranscriptChunk."""
        return speech_pb2.TranscriptChunk(
            start_time=segment.start,
            end_time=segment.end,
            text=segment.text.strip(),
            speaker_id=speaker_id,
            confidence=segment.avg_logprob,
            words=[
                speech_pb2.Word(
                    start_time=w.start,
                    end_time=w.end,
                    text=w.word,
                    confidence=w.probability,
                )
                for w in (segment.words or [])
            ],
        )

    # ── Main RPC ──────────────────────────────────────────────

    def Transcribe(self, request, context):
        """
        Stream transcription chunks for the provided audio.

        When diarization is requested, the diarization pipeline runs
        first on the full audio.  Then Whisper streams segments, each
        assigned a speaker label on-the-fly via temporal overlap.

        Yields:
            speech_pb2.TranscriptChunk
        """
        inf = self.inference
        prompt = request.options.initial_prompt or inf.initial_prompt or None

        # ── Resolve audio source ─────────────────────────────
        try:
            audio, log_source, source_type = self._resolver.resolve(request)
        except (AudioValidationError, AudioFetchError) as exc:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, str(exc))
            return

        # ── Validate diarization request ─────────────────────
        diarize = request.options.diarization
        if diarize and self.diarizer is None:
            context.abort(
                grpc.StatusCode.INVALID_ARGUMENT,
                "Diarization requested but not enabled in server "
                "configuration. Set [diarization] enabled = true in "
                "config.toml.",
            )
            return

        # ── Preprocess audio ─────────────────────────────────
        try:
            whisper_input, diar_input = self._preprocessor.prepare(
                audio,
                diarize=diarize,
            )
        except AudioDecodeError as exc:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, str(exc))
            return

        # ── Phase 1: Diarization (if requested) ──────────────
        diarization: DiarizationResult | None = None
        if diarize and diar_input is not None:
            logger.info(
                "Running diarization (phase 1 of 2)",
                extra={
                    "audio_source_type": source_type,
                    "audio_source": log_source,
                },
            )
            try:
                diarization = self.diarizer.run(
                    diar_input,
                    min_speakers=self._get_num_speakers(request.options),
                    max_speakers=self._get_num_speakers(request.options),
                )
            except _INFERENCE_ERRORS:
                logger.exception(
                    "Diarization pipeline failed",
                    extra={
                        "audio_source_type": source_type,
                        "audio_source": log_source,
                    },
                )
                context.abort(
                    grpc.StatusCode.INTERNAL,
                    "Diarization pipeline failed. Check server logs for details.",
                )
                return

            logger.info(
                "Diarization complete, starting transcription stream",
                extra={
                    "num_speakers": len(diarization.labels()),
                },
            )

        # ── Phase 2: Whisper streaming ────────────────────────
        try:
            segments, info = self.model.transcribe(
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
            logger.exception(
                "Whisper transcription failed to initialise",
                extra={
                    "audio_source_type": source_type,
                    "audio_source": log_source,
                },
            )
            context.abort(
                grpc.StatusCode.INTERNAL,
                "Transcription failed to initialise. Check server logs for details.",
            )
            return

        logger.info(
            "Transcription streaming started",
            extra={
                "language": info.language,
                "language_probability": round(info.language_probability, 2),
                "audio_source_type": source_type,
                "audio_source": log_source,
                "diarization": diarization is not None,
            },
        )

        try:
            for segment in segments:
                if diarization is not None:
                    speaker = diarization.speaker_at(
                        segment.start,
                        segment.end,
                    )
                else:
                    speaker = ""

                yield self._segment_to_chunk(segment, speaker_id=speaker)
        except grpc.RpcError:
            raise
        except _INFERENCE_ERRORS:
            logger.exception(
                "Whisper transcription failed mid-stream",
                extra={
                    "audio_source_type": source_type,
                    "audio_source": log_source,
                },
            )
            context.abort(
                grpc.StatusCode.INTERNAL,
                "Transcription failed mid-stream. Check server logs for details.",
            )
            return
