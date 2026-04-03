"""
gRPC Servicer for the Whisper Speech-to-Text service.

This module coordinates the transcription flow:
1. resolve and preprocess audio.
2. optionally run diarization.
3. transcribe with Whisper.
4. stream transcript chunks back to the caller.
"""

import logging
from concurrent.futures import ThreadPoolExecutor, Future
from typing import Optional

import grpc
from faster_whisper import WhisperModel

from audio import (
    AudioResolver,
    AudioPreprocessor,
    AudioValidationError,
    AudioFetchError,
    AudioDecodeError,
)
from settings import Settings
from speech import speech_pb2_grpc, speech_pb2

logger = logging.getLogger(__name__)

_INFERENCE_ERRORS = (RuntimeError, ValueError, OSError)


def _load_diarization():
    """
    Lazily import diarization dependencies.

    This keeps the service usable when diarization support is not installed
    or not enabled in the runtime environment.
    """
    try:
        from diarization import DiarizationPipeline, DiarizationResult

        return DiarizationPipeline, DiarizationResult
    except Exception as e:
        raise RuntimeError("Diarization dependencies are not installed.") from e


class WhisperServicer(speech_pb2_grpc.SpeechToTextServicer):
    """
    gRPC service implementation for streaming speech-to-text responses.

    The servicer owns:
    - audio resolution and validation,
    - Whisper model inference,
    - optional speaker diarization,
    - conversion of internal segments into protobuf messages.
    """

    def __init__(self, settings: Settings):
        """
        Initialize the service with runtime settings.

        Diarization is intentionally initialized lazily so the service can
        start even when diarization dependencies are missing.
        """
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

        self._diarization_config = s.diarization
        self.diarizer = None

        self._executor = ThreadPoolExecutor(max_workers=2)

    # ── Helpers ───────────────────────────────────────────────

    @staticmethod
    def _get_num_speakers(options) -> int:
        """
        Return the requested speaker count from request options.

        If the field is absent, returns 0 so the diarizer can use defaults.
        """
        if options.HasField("num_speakers"):
            return options.num_speakers
        return 0

    @staticmethod
    def _segment_to_chunk(segment, speaker_id: str = "") -> speech_pb2.TranscriptChunk:
        """
        Convert an entire Whisper segment into the protobuf transcript format.
        """
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

    @staticmethod
    def _words_to_chunk(
        words: list, speaker_id: str, confidence: float
    ) -> speech_pb2.TranscriptChunk:
        """
        Helper to create a protobuf transcript chunk from a sub-list of words.
        This allows us to split a single Whisper segment across multiple speakers.
        """
        return speech_pb2.TranscriptChunk(
            start_time=words[0].start,
            end_time=words[-1].end,
            text="".join(w.word for w in words).strip(),
            speaker_id=speaker_id,
            confidence=confidence,
            words=[
                speech_pb2.Word(
                    start_time=w.start,
                    end_time=w.end,
                    text=w.word,
                    confidence=w.probability,
                )
                for w in words
            ],
        )

    # ── Main RPC ──────────────────────────────────────────────

    def Transcribe(self, request, context):
        """
        Stream a transcription response for the provided audio request.

        When diarization is enabled, speaker labels are computed first and then
        applied to each Whisper segment (or sub-segment) in the response stream.
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
                    "Diarization requested but not enabled in config.",
                )
                return

            if self.diarizer is None:
                diarization_pipeline, _ = _load_diarization()
                self.diarizer = diarization_pipeline(self._diarization_config)

        # ── Preprocess ────────────────────────────────
        try:
            whisper_input, diar_input = self._preprocessor.prepare(
                audio,
                diarize=diarize,
            )
        except AudioDecodeError as exc:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, str(exc))
            return

        # ── Phase 1: Background Diarization ───────────
        diarization_future: Optional[Future] = None
        if diarize and diar_input is not None:
            local_diarizer = self.diarizer
            assert local_diarizer is not None, (
                "Diarizer must be initialized before running."
            )

            diarization_future = self._executor.submit(
                local_diarizer.run,
                diar_input,
                min_speakers=self._get_num_speakers(request.options),
                max_speakers=self._get_num_speakers(request.options),
            )

        # ── Phase 2: Whisper ──────────────────────────
        try:
            # Force word_timestamps to True if diarization is enabled
            # so we can perform word-level speaker alignment to prevent bleed.
            use_word_timestamps = inf.word_timestamps or diarize

            segments, info = self.model.transcribe(
                whisper_input,
                language=request.language or None,
                beam_size=inf.beam_size,
                vad_filter=inf.vad_filter,
                vad_parameters={
                    "min_silence_duration_ms": inf.vad_min_silence_ms,
                },
                word_timestamps=use_word_timestamps,
                initial_prompt=prompt,
                no_speech_threshold=inf.no_speech_threshold,
                log_prob_threshold=inf.log_prob_threshold,
                compression_ratio_threshold=inf.compression_ratio_threshold,
            )
        except _INFERENCE_ERRORS:
            logger.exception("Whisper failed")
            context.abort(grpc.StatusCode.INTERNAL, "Transcription failed")
            return

        # ── Phase 3: Wait for Diarization ─────────────
        diarization = None
        if diarization_future is not None:
            try:
                segments = list(segments)
                diarization = diarization_future.result()
            except _INFERENCE_ERRORS:
                logger.exception("Diarization failed")
                context.abort(
                    grpc.StatusCode.INTERNAL,
                    "Diarization failed",
                )
                return

        try:
            for segment in segments:
                # Fallback to segment-level if diarization is missing or no word timestamps
                if diarization is None or not segment.words:
                    speaker = (
                        diarization.speaker_at(segment.start, segment.end)
                        if diarization
                        else ""
                    )
                    yield self._segment_to_chunk(segment, speaker_id=speaker)
                    continue

                # Splitting segment by word-level speakers
                segment_dominant_speaker = diarization.speaker_at(
                    segment.start, segment.end
                )
                current_speaker = None
                current_words = []

                for w in segment.words:
                    word_speaker = diarization.speaker_at(w.start, w.end)

                    # Fallback if no specific speaker turn is found for this exact word duration
                    if not word_speaker:
                        word_speaker = current_speaker or segment_dominant_speaker or ""

                    if current_speaker is None:
                        current_speaker = word_speaker

                    # A speaker boundary was crossed within the same Whisper segment
                    if word_speaker != current_speaker:
                        if current_words:
                            yield self._words_to_chunk(
                                current_words,
                                speaker_id=current_speaker or "",
                                confidence=segment.avg_logprob,
                            )
                        current_speaker = word_speaker
                        current_words = [w]
                    else:
                        current_words.append(w)

                # Yield any remaining accumulated words for this segment
                if current_words:
                    yield self._words_to_chunk(
                        current_words,
                        speaker_id=current_speaker or "",
                        confidence=segment.avg_logprob,
                    )

        except grpc.RpcError:
            raise
        except _INFERENCE_ERRORS:
            logger.exception("Streaming failed")
            context.abort(grpc.StatusCode.INTERNAL, "Streaming failed")
