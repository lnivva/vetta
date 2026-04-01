"""
Speaker diarization pipeline using pyannote.audio.

Provides speaker label assignment to transcription segments by finding
the dominant speaker within each segment's time boundaries using
maximum temporal overlap.
"""

import io
import logging
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from settings import DiarizationConfig

logger = logging.getLogger(__name__)

if TYPE_CHECKING:
    pass


@dataclass
class SpeakerSegment:
    """A speaker-labeled time range extracted from diarization output."""

    start: float
    end: float
    speaker: str


class DiarizationResult:
    """Lightweight wrapper around a pyannote `Annotation` result."""

    __slots__ = ("_annotation",)

    def __init__(self, annotation: Any):
        self._annotation = annotation

    def labels(self) -> list[str]:
        """Return the discovered speaker labels as strings."""
        return [str(label) for label in self._annotation.labels()]

    def speaker_at(self, start: float, end: float) -> str:
        """
        Return the speaker with the greatest overlap for the given time span.

        If no overlapping speaker turn is found, returns an empty string.
        """
        from pyannote.core import Segment

        target = Segment(start, end)
        best_speaker = ""
        best_overlap = 0.0

        for turn, _, speaker in self._annotation.itertracks(yield_label=True):
            overlap = target & turn
            if overlap is not None:
                overlap_duration = overlap.end - overlap.start
                if overlap_duration > best_overlap:
                    best_overlap = overlap_duration
                    best_speaker = speaker

        return best_speaker

    def assign_speakers(self, segments: list[dict]) -> list[dict]:
        """
        Annotate transcript segments and nested words with speaker labels.

        Each segment and word dictionary is expected to contain `start` and
        `end` keys (or `start_time` and `end_time`).  A `speaker` (and
        optionally `speaker_id`) key will be added in-place.

        Supports both naming conventions so that raw Whisper-style dicts
        (`start`/`end`) and servicer-normalised dicts (`start_time`/`end_time`)
        are handled transparently.
        """
        for seg in segments:
            seg_start = seg.get("start", seg.get("start_time", 0.0))
            seg_end = seg.get("end", seg.get("end_time", seg_start))
            speaker = self.speaker_at(seg_start, seg_end)
            seg["speaker"] = speaker
            seg["speaker_id"] = speaker

            for word in seg.get("words", []):
                w_start = word.get("start", word.get("start_time", 0.0))
                w_end = word.get("end", word.get("end_time", w_start))
                word_speaker = self.speaker_at(w_start, w_end)
                word["speaker"] = word_speaker
                word["speaker_id"] = word_speaker

        return segments


class DiarizationPipeline:
    """Lazy-loading wrapper for the pyannote diarization pipeline."""

    def __init__(self, config: DiarizationConfig):
        if not config.hf_token:
            raise ValueError("Diarization enabled but no HuggingFace token configured.")

        logger.info(
            "Loading diarization pipeline",
            extra={"model": config.model, "device": config.device},
        )

        # Lazy imports to avoid requiring heavy dependencies unless used.
        try:
            import torch
            from pyannote.audio import Pipeline
        except Exception as e:
            raise RuntimeError(
                "Diarization dependencies are not installed. "
                "Install pyannote.audio and torch stack."
            ) from e

        pipeline = Pipeline.from_pretrained(
            config.model,
            token=config.hf_token,
        )

        if pipeline is None:
            raise RuntimeError(f"Failed to load diarization pipeline '{config.model}'")

        # Device handling.
        if config.device == "cuda":
            if not torch.cuda.is_available():
                raise RuntimeError("CUDA requested but not available")
            pipeline = pipeline.to(torch.device("cuda"))

        elif config.device == "mps":
            if not torch.backends.mps.is_available():
                raise RuntimeError("MPS requested but not available")
            pipeline = pipeline.to(torch.device("mps"))

        else:
            pipeline = pipeline.to(torch.device("cpu"))

        self.pipeline = pipeline
        self.default_min_speakers = config.min_speakers
        self.default_max_speakers = config.max_speakers

        logger.info("Diarization pipeline loaded successfully")

    @staticmethod
    def _extract_annotation(result: Any):
        """Extract a pyannote `Annotation` from a pipeline result object."""
        from pyannote.core import Annotation

        if isinstance(result, Annotation):
            return result

        if hasattr(result, "speaker_diarization"):
            return result.speaker_diarization

        raise TypeError(f"Cannot extract Annotation from {type(result).__name__}")

    def run(
        self,
        audio_input: str | io.BytesIO,
        min_speakers: int = 0,
        max_speakers: int = 0,
    ) -> DiarizationResult:
        """
        Run diarization on an audio file path or in-memory audio buffer.

        The explicit `min_speakers` and `max_speakers` values override the
        defaults from configuration when provided.
        """
        if isinstance(audio_input, io.BytesIO):
            audio_input.seek(0)

        effective_min = min_speakers or self.default_min_speakers
        effective_max = max_speakers or self.default_max_speakers

        if effective_min > 0 and 0 < effective_max < effective_min:
            raise ValueError("min_speakers cannot be greater than max_speakers")

        if effective_min < 0 or effective_max < 0:
            raise ValueError("speakers must be >= 0")

        logger.debug(
            "Running diarization",
            extra={"min_speakers": effective_min, "max_speakers": effective_max},
        )

        result = self.pipeline(
            audio_input,
            min_speakers=effective_min,
            max_speakers=effective_max,
        )

        annotation = self._extract_annotation(result)

        logger.info(
            "Diarization complete",
            extra={"num_speakers": len(annotation.labels())},
        )

        return DiarizationResult(annotation)
