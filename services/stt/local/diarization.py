"""
Speaker diarization pipeline using pyannote.audio.

Provides speaker label assignment to transcription segments by finding
the dominant speaker within each segment's time boundaries using
maximum temporal overlap.
"""

import io
import logging
from dataclasses import dataclass

import torch
from pyannote.audio import Pipeline
from pyannote.core import Annotation, Segment

from settings import DiarizationConfig

logger = logging.getLogger(__name__)


@dataclass
class SpeakerSegment:
    """A time-bounded region with an assigned speaker label."""

    start: float
    end: float
    speaker: str


class DiarizationResult:
    """
    Immutable view over a pyannote diarization annotation.

    Exposes only the operations the rest of the application needs,
    keeping pyannote types out of the public interface.
    """

    __slots__ = ("_annotation",)

    def __init__(self, annotation: Annotation):
        self._annotation = annotation

    def labels(self) -> list[str]:
        """Return the discovered speaker labels."""
        return [str(label) for label in self._annotation.labels()]

    def speaker_at(self, start: float, end: float) -> str:
        """
        Return the dominant speaker for a time span.

        Finds the speaker with the greatest temporal overlap in
        [start, end].  Returns "" if no speaker overlaps the interval.

        Parameters:
            start: Interval start time in seconds.
            end:   Interval end time in seconds.

        Returns:
            Speaker label (e.g. "SPEAKER_00"), or "".
        """
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
        Assign a speaker label to each transcript segment based on
        maximum temporal overlap with the diarization output.

        Each dict in ``segments`` must contain ``"start"`` and ``"end"``
        keys (floats, in seconds).  After this call each dict will also
        contain a ``"speaker"`` key.

        For word-level assignment, each dict may optionally contain a
        ``"words"`` key (list of dicts with ``"start"`` and ``"end"``),
        and each word dict will also receive a ``"speaker"`` key.

        Parameters:
            segments: Transcript segments to label.

        Returns:
            The same list, now with ``"speaker"`` keys populated.
        """
        for seg in segments:
            seg["speaker"] = self.speaker_at(seg["start"], seg["end"])

            for word in seg.get("words", []):
                word["speaker"] = self.speaker_at(word["start"], word["end"])

        return segments


class DiarizationPipeline:
    """
    Wraps pyannote's speaker-diarization pipeline.

    Loads the model once at startup and exposes a ``run()`` method that
    returns a :class:`DiarizationResult` for a given audio input.

    Attributes:
        pipeline (Pipeline): The loaded pyannote diarization pipeline.
        default_min_speakers (int): Default minimum speakers (0 = auto).
        default_max_speakers (int): Default maximum speakers (0 = auto).
    """

    pipeline: Pipeline
    default_min_speakers: int
    default_max_speakers: int

    def __init__(self, config: DiarizationConfig):
        """
        Load the pyannote diarization pipeline.

        Parameters:
            config: Diarization settings including model name,
                HuggingFace token, device, and speaker bounds.

        Raises:
            ValueError: If enabled but hf_token is empty.
            RuntimeError: If the pipeline fails to load.
        """
        if not config.hf_token:
            raise ValueError(
                "Diarization is enabled but no HuggingFace token is configured. "
                "Set [diarization] hf_token in config.toml or the "
                "WHISPER_DIARIZATION_HF_TOKEN environment variable. "
                "You must also accept the model license at "
                "https://huggingface.co/pyannote/speaker-diarization-3.1"
            )

        logger.info(
            "Loading diarization pipeline",
            extra={"model": config.model, "device": config.device},
        )

        pipeline = Pipeline.from_pretrained(
            config.model,
            token=config.hf_token,
        )

        if pipeline is None:
            raise RuntimeError(
                f"Failed to load diarization pipeline '{config.model}'. "
                f"Pipeline.from_pretrained() returned None. "
                f"Verify the model name and that your HuggingFace token "
                f"has access to the model."
            )

        if config.device == "cuda":
            if not torch.cuda.is_available():
                raise RuntimeError(
                    "Diarization device set to 'cuda' but CUDA is unavailable"
                )
            pipeline = pipeline.to(torch.device("cuda"))
        elif config.device == "mps":
            if not torch.backends.mps.is_available():
                raise RuntimeError(
                    "Diarization device set to 'mps' but MPS is unavailable"
                )
            pipeline = pipeline.to(torch.device("mps"))
        else:
            pipeline = pipeline.to(torch.device("cpu"))

        self.pipeline = pipeline
        self.default_min_speakers = config.min_speakers
        self.default_max_speakers = config.max_speakers

        logger.info("Diarization pipeline loaded successfully")

    @staticmethod
    def _extract_annotation(result: object) -> Annotation:
        """
        Extract an Annotation from the pipeline result.

        Parameters:
            result: The raw output from the pyannote pipeline.

        Returns:
            The diarization annotation.
        """
        if isinstance(result, Annotation):
            return result

            # pyannote.audio >= 3.x returns DiarizeOutput dataclass
        if hasattr(result, "speaker_diarization"):
            return result.speaker_diarization  # type: ignore[no-any-return]

        raise TypeError(
            f"Cannot extract Annotation from {type(result).__name__}. "
            f"Available attributes: {dir(result)}"
        )

    def run(
        self,
        audio_input: str | io.BytesIO,
        min_speakers: int = 0,
        max_speakers: int = 0,
    ) -> DiarizationResult:
        """
        Run speaker diarization on the provided audio.

        Parameters:
            audio_input: A file path or BytesIO containing audio data.
            min_speakers: Minimum expected speakers.  0 uses the
                configured default; if that is also 0, pyannote decides.
            max_speakers: Maximum expected speakers.  Same logic.

        Returns:
            A :class:`DiarizationResult` that provides speaker lookup
            without exposing pyannote internals.
        """

        if isinstance(audio_input, io.BytesIO):
            audio_input.seek(0)

        effective_min = min_speakers or self.default_min_speakers
        effective_max = max_speakers or self.default_max_speakers

        if effective_min > 0 and 0 < effective_max < effective_min:
            raise ValueError(
                f"min_speakers ({effective_min}) cannot be greater than "
                f"max_speakers ({effective_max})"
            )

        if effective_min < 0 or effective_max < 0:
            raise ValueError(
                f"min_speakers ({effective_min}) and max_speakers ({effective_max}) must be >= 0"
            )

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

        speaker_count = len(annotation.labels())
        logger.info(
            "Diarization complete",
            extra={"num_speakers": speaker_count},
        )

        return DiarizationResult(annotation)
