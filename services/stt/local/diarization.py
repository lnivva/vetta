"""
Speaker diarization pipeline using pyannote.audio.

Provides speaker label assignment to transcription segments by finding
the dominant speaker within each segment's time boundaries using
maximum temporal overlap.

Error philosophy
----------------
This module never silently degrades. Every failure path either raises an
exception or logs at ERROR level. When diarization is configured as
required, failures are always fatal.
"""

import io
import logging
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any

from settings import DiarizationConfig

logger = logging.getLogger(__name__)

if TYPE_CHECKING:
    pass


class DiarizationError(Exception):
    """Raised when the diarization pipeline fails irrecoverably."""


class DiarizationInitError(DiarizationError):
    """Raised when the diarization pipeline cannot be initialized."""


class DiarizationRuntimeError(DiarizationError):
    """Raised when a diarization run fails after successful init."""


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
        if annotation is None:
            raise DiarizationRuntimeError(
                "Cannot create DiarizationResult from None annotation"
            )
        self._annotation = annotation

    def labels(self) -> list[str]:
        """Return the discovered speaker labels as strings."""
        return [str(label) for label in self._annotation.labels()]

    def speaker_at(self, start: float, end: float) -> str:
        """
        Return the speaker with the greatest overlap for the given time span.

        If no overlapping speaker turn is found, returns an empty string and
        logs a debug-level message (this is expected for silence regions).
        """
        from pyannote.core import Segment

        if start >= end:
            logger.warning(
                "Invalid time range for speaker lookup: start=%.3f >= end=%.3f",
                start,
                end,
            )
            return ""

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

        if not best_speaker:
            logger.debug(
                "No speaker found for range [%.3f, %.3f] — likely silence",
                start,
                end,
            )

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
        if not segments:
            logger.warning("assign_speakers called with empty segment list")
            return segments

        speakers_found = 0

        for seg in segments:
            seg_start = seg.get("start", seg.get("start_time", 0.0))
            seg_end = seg.get("end", seg.get("end_time", seg_start))
            speaker = self.speaker_at(seg_start, seg_end)
            seg["speaker"] = speaker
            seg["speaker_id"] = speaker

            if speaker:
                speakers_found += 1

            for word in seg.get("words", []):
                w_start = word.get("start", word.get("start_time", 0.0))
                w_end = word.get("end", word.get("end_time", w_start))
                word_speaker = self.speaker_at(w_start, w_end)
                word["speaker"] = word_speaker
                word["speaker_id"] = word_speaker

        if speakers_found == 0:
            logger.error(
                "Diarization produced zero speaker assignments across %d segments. "
                "The diarization model may have failed silently or the audio "
                "contains no detectable speech.",
                len(segments),
            )

        logger.info(
            "Speaker assignment complete: %d/%d segments assigned",
            speakers_found,
            len(segments),
        )

        return segments


class DiarizationPipeline:
    """Lazy-loading wrapper for the pyannote diarization pipeline."""

    def __init__(self, config: DiarizationConfig):
        if not config.hf_token:
            raise DiarizationInitError(
                "Diarization enabled but no HuggingFace token configured. "
                "Set the hf_token in DiarizationConfig."
            )

        logger.info(
            "Loading diarization pipeline",
            extra={"model": config.model, "device": config.device},
        )

        try:
            import torch
            from pyannote.audio import Pipeline
        except ImportError as e:
            raise DiarizationInitError(
                "Diarization dependencies are not installed. "
                "Install with: pip install pyannote.audio torch torchaudio"
            ) from e

        try:
            pipeline = Pipeline.from_pretrained(
                config.model,
                token=config.hf_token,
            )
        except Exception as e:
            raise DiarizationInitError(
                f"Failed to load diarization model '{config.model}'. "
                f"Verify the model name and that your HuggingFace token has "
                f"accepted the model's license agreement at "
                f"https://huggingface.co/{config.model}"
            ) from e

        if pipeline is None:
            raise DiarizationInitError(
                f"Pipeline.from_pretrained returned None for '{config.model}'. "
                f"This usually means the HuggingFace token lacks access to the "
                f"gated model. Accept the license at "
                f"https://huggingface.co/{config.model}"
            )

            # ── Device placement ─────────────────────────
        try:
            import torch as _torch

            if config.device == "cuda":
                if not _torch.cuda.is_available():
                    raise DiarizationInitError(
                        "CUDA requested for diarization but no CUDA device is available"
                    )
                pipeline = pipeline.to(_torch.device("cuda"))
                logger.info("Diarization pipeline placed on CUDA")

            elif config.device == "mps":
                if not _torch.backends.mps.is_available():
                    raise DiarizationInitError(
                        "MPS requested for diarization but MPS is not available"
                    )
                pipeline = pipeline.to(_torch.device("mps"))
                logger.info("Diarization pipeline placed on MPS")

            else:
                pipeline = pipeline.to(_torch.device("cpu"))
                logger.info("Diarization pipeline placed on CPU")

        except DiarizationInitError:
            raise
        except Exception as e:
            raise DiarizationInitError(
                f"Failed to move diarization pipeline to device '{config.device}'"
            ) from e

        self.pipeline = pipeline
        self.default_min_speakers = config.min_speakers
        self.default_max_speakers = config.max_speakers

        logger.info("Diarization pipeline loaded successfully")

    @staticmethod
    def _extract_annotation(result: Any):
        """
        Extract a pyannote `Annotation` from a pipeline result object.

        Raises DiarizationRuntimeError if the result cannot be interpreted.
        """
        from pyannote.core import Annotation

        if isinstance(result, Annotation):
            return result

        if hasattr(result, "speaker_diarization"):
            ann = result.speaker_diarization
            if isinstance(ann, Annotation):
                return ann

        raise DiarizationRuntimeError(
            f"Cannot extract Annotation from pipeline result of type "
            f"{type(result).__name__}. The pyannote pipeline returned an "
            f"unexpected object."
        )

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

        Raises:
            DiarizationRuntimeError: On any failure during the diarization run.
            ValueError: On invalid speaker count parameters.
        """
        if isinstance(audio_input, io.BytesIO):
            audio_input.seek(0)
            if audio_input.getbuffer().nbytes == 0:
                raise DiarizationRuntimeError(
                    "Empty audio buffer passed to diarization pipeline"
                )

        effective_min = min_speakers or self.default_min_speakers
        effective_max = max_speakers or self.default_max_speakers

        if effective_min > 0 and 0 < effective_max < effective_min:
            raise ValueError(
                f"min_speakers ({effective_min}) cannot be greater than "
                f"max_speakers ({effective_max})"
            )

        if effective_min < 0 or effective_max < 0:
            raise ValueError(
                f"Speaker counts must be >= 0, got "
                f"min_speakers={effective_min}, max_speakers={effective_max}"
            )

        logger.info(
            "Running diarization",
            extra={"min_speakers": effective_min, "max_speakers": effective_max},
        )

        try:
            result = self.pipeline(
                audio_input,
                min_speakers=effective_min,
                max_speakers=effective_max,
            )
        except Exception as e:
            raise DiarizationRuntimeError(
                f"Diarization pipeline execution failed: {e}"
            ) from e

        try:
            annotation = self._extract_annotation(result)
        except DiarizationRuntimeError:
            raise
        except Exception as e:
            raise DiarizationRuntimeError(
                f"Failed to extract annotation from pipeline result: {e}"
            ) from e

        num_speakers = len(annotation.labels())

        if num_speakers == 0:
            logger.error(
                "Diarization completed but found ZERO speakers. "
                "The audio may contain no detectable speech, or the "
                "pipeline may have failed silently."
            )

        logger.info(
            "Diarization complete",
            extra={"num_speakers": num_speakers},
        )

        return DiarizationResult(annotation)
