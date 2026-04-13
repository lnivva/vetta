import logging
import threading
from concurrent.futures import ThreadPoolExecutor, Future
from typing import Any, Optional, Iterator

from faster_whisper import WhisperModel

from src.core.audio import AudioPreprocessor
from src.core.settings import Settings
from src.stt.domain import TranscriptChunkResult, WordSegment

logger = logging.getLogger(__name__)

INFERENCE_ERRORS = (RuntimeError, ValueError, OSError)


def _load_diarization():
    """
    Lazily import diarization dependencies.

    This keeps the service usable when diarization support is not installed
    or not enabled in the runtime environment.
    """
    try:
        from src.diarization.pipeline import DiarizationPipeline, DiarizationResult

        return DiarizationPipeline, DiarizationResult
    except ImportError as e:
        raise RuntimeError("Diarization dependencies are not installed.") from e


class TranscriptionEngine:
    """
    Core business logic for Speech-to-Text and Diarization.
    Isolated from any network/gRPC transport layers.
    """

    def __init__(self, settings: Settings):
        s = settings
        self.inference = s.inference
        self._preprocessor = AudioPreprocessor()

        # Initialize Whisper natively
        self.model = WhisperModel(
            s.model.size,
            device=s.model.device,
            compute_type=s.model.compute_type,
            download_root=s.model.download_dir,
            num_workers=s.concurrency.num_workers,
            cpu_threads=s.concurrency.cpu_threads,
        )

        self._diarization_config = s.diarization
        self.diarizer: Any = None
        self._diarizer_lock = threading.Lock()

        # Executor dedicated to running the Pyannote pipeline in the background
        self._executor = ThreadPoolExecutor(max_workers=2)

    def transcribe(
        self,
        audio_data: bytes,
        diarize: bool = False,
        num_speakers: int = 0,
        language: Optional[str] = None,
        initial_prompt: Optional[str] = None,
    ) -> Iterator[TranscriptChunkResult]:
        """
        Executes the STT pipeline.
        Yields domain dataclasses representing streaming transcript chunks.
        """
        inf = self.inference
        prompt = initial_prompt or inf.initial_prompt or None

        if diarize and not self._diarization_config.enabled:
            raise ValueError("Diarization requested but not enabled in config.")

        if diarize and self.diarizer is None:
            with self._diarizer_lock:
                if self.diarizer is None:
                    diarization_pipeline, _ = _load_diarization()
                    self.diarizer = diarization_pipeline(self._diarization_config)

        # ── Preprocess ────────────────────────────────
        whisper_input, diar_input = self._preprocessor.prepare(
            audio_data,
            diarize=diarize,
        )

        # ── Phase 1: Background Diarization ───────────
        diarization_future: Optional[Future] = None
        if diarize and diar_input is not None:
            assert self.diarizer is not None, (
                "Diarizer must be initialized when diarize=True"
            )
            # Dispatch Pyannote to run independently in a separate thread
            diarization_future = self._executor.submit(
                self.diarizer.run,
                diar_input,
                min_speakers=num_speakers,
                max_speakers=num_speakers,
            )

        # ── Phase 2: Whisper ──────────────────────────
        # Whisper runs in the main thread concurrently with diarization
        use_word_timestamps = inf.word_timestamps or diarize
        segments, info = self.model.transcribe(
            whisper_input,
            language=language,
            beam_size=inf.beam_size,
            vad_filter=inf.vad_filter,
            vad_parameters={"min_silence_duration_ms": inf.vad_min_silence_ms},
            word_timestamps=use_word_timestamps,
            initial_prompt=prompt,
            no_speech_threshold=inf.no_speech_threshold,
            log_prob_threshold=inf.log_prob_threshold,
            compression_ratio_threshold=inf.compression_ratio_threshold,
        )

        # ── Phase 3: Wait & Merge ─────────────────────
        diarization_timeout_seconds = 60 * 60
        diarization = None
        if diarization_future is not None:
            try:
                diarization = diarization_future.result(
                    timeout=diarization_timeout_seconds
                )
            except TimeoutError:
                logger.error(
                    "Diarization timed out after %ds", diarization_timeout_seconds
                )
                diarization_future.cancel()

        for segment in segments:
            if diarization is None or not segment.words:
                speaker = (
                    diarization.speaker_at(segment.start, segment.end)
                    if diarization
                    else ""
                )
                yield self._build_chunk(segment, speaker)
                continue

            segment_dominant_speaker = diarization.speaker_at(
                segment.start, segment.end
            )
            current_speaker: Optional[str] = None
            current_words: list[Any] = []

            for w in segment.words:
                word_speaker = (
                    diarization.speaker_at(w.start, w.end)
                    or current_speaker
                    or segment_dominant_speaker
                    or ""
                )

                if current_speaker is None:
                    current_speaker = word_speaker

                if word_speaker != current_speaker:
                    if current_words:
                        yield self._build_words_chunk(
                            current_words, current_speaker or "", segment.avg_logprob
                        )
                    current_speaker = word_speaker
                    current_words = [w]
                else:
                    current_words.append(w)

            if current_words:
                yield self._build_words_chunk(
                    current_words, current_speaker or "", segment.avg_logprob
                )

    @staticmethod
    def _build_chunk(segment, speaker_id: str) -> TranscriptChunkResult:
        words = [
            WordSegment(
                start_time=w.start,
                end_time=w.end,
                text=w.word,
                confidence=w.probability,
                speaker_id=speaker_id,
            )
            for w in (segment.words or [])
        ]
        return TranscriptChunkResult(
            start_time=segment.start,
            end_time=segment.end,
            text=segment.text.strip(),
            speaker_id=speaker_id,
            confidence=segment.avg_logprob,
            words=words,
        )

    @staticmethod
    def _build_words_chunk(
        words: list, speaker_id: str, confidence: float
    ) -> TranscriptChunkResult:
        if not words:
            raise ValueError("Cannot build chunk from empty words list")

        word_segments = [
            WordSegment(
                start_time=w.start,
                end_time=w.end,
                text=w.word,
                confidence=w.probability,
                speaker_id=speaker_id,
            )
            for w in words
        ]
        return TranscriptChunkResult(
            start_time=words[0].start,
            end_time=words[-1].end,
            text="".join(w.word for w in words).strip(),
            speaker_id=speaker_id,
            confidence=confidence,
            words=word_segments,
        )
