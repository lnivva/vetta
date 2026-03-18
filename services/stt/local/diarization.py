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

from settings import DiarizationConfig, Device

logger = logging.getLogger(__name__)


@dataclass
class SpeakerSegment:
    """A time-bounded region with an assigned speaker label."""

    start: float
    end: float
    speaker: str


class DiarizationPipeline:
    """
    Wraps pyannote's speaker-diarization pipeline.

    Loads the model once at startup and exposes a `run()` method that
    returns speaker assignments for a given audio input, and a static
    `assign_speakers()` that merges diarization output with transcript
    segments.

    Attributes:
        pipeline (Pipeline): The loaded pyannote diarization pipeline.
        default_min_speakers (int): Default minimum speakers (0 = auto).
        default_max_speakers (int): Default maximum speakers (0 = auto).
    """

    def __init__(self, config: DiarizationConfig):
        """
        Load the pyannote diarization pipeline.

        Parameters:
            config (DiarizationConfig): Diarization settings including
                model name, HuggingFace token, device, and speaker bounds.

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

        self.pipeline = Pipeline.from_pretrained(
            config.model,
            token=config.hf_token,
        )

        # Move to the appropriate device
        if config.device == "cuda" and torch.cuda.is_available():
            self.pipeline.to(torch.device("cuda"))
        else:
            self.pipeline.to(torch.device("cpu"))

        self.default_min_speakers = config.min_speakers
        self.default_max_speakers = config.max_speakers

        logger.info("Diarization pipeline loaded successfully")

    @staticmethod
    def _extract_annotation(result) -> Annotation:
        """
        Extract an Annotation from the pipeline result.

        Parameters:
            result: The raw output from the pyannote pipeline.

        Returns:
            pyannote.core.Annotation: The diarization annotation.
        """
        if isinstance(result, Annotation):
            return result

            # pyannote.audio >= 3.x returns DiarizeOutput dataclass
        if hasattr(result, "speaker_diarization"):
            return result.speaker_diarization

        raise TypeError(
            f"Cannot extract Annotation from {type(result).__name__}. "
            f"Available attributes: {dir(result)}"
        )

    def run(
            self,
            audio_input: str | io.BytesIO,
            min_speakers: int = 0,
            max_speakers: int = 0,
    ) -> Annotation:
        """
        Run speaker diarization on the provided audio.

        Parameters:
            audio_input: A file path (str) or BytesIO containing audio data.
            min_speakers (int): Minimum expected speakers. 0 uses the
                configured default; if that is also 0, pyannote decides.
            max_speakers (int): Maximum expected speakers. Same logic as above.

        Returns:
            pyannote.core.Annotation: Speaker diarization results with
                labeled time segments.
        """
        if isinstance(audio_input, io.BytesIO):
            audio_input.seek(0)

            # Resolve speaker bounds: request override > config default > omit
        params: dict = {}
        effective_min = min_speakers or self.default_min_speakers
        effective_max = max_speakers or self.default_max_speakers

        if effective_min > 0:
            params["min_speakers"] = effective_min
        if effective_max > 0:
            params["max_speakers"] = effective_max

        logger.debug("Running diarization", extra={"params": params})
        result = self.pipeline(audio_input, **params)

        # Handle both old (Annotation) and new (DiarizeOutput) return types
        annotation = self._extract_annotation(result)

        speaker_count = len(annotation.labels())
        logger.info(
            "Diarization complete",
            extra={"num_speakers": speaker_count},
        )

        return annotation

    @staticmethod
    def assign_speakers(
            diarization: Annotation,
            segments: list[dict],
    ) -> list[dict]:
        """
        Assign a speaker label to each transcript segment based on
        maximum temporal overlap with the diarization output.

        Each dict in `segments` must contain `"start"` and `"end"` keys
        (floats, in seconds). After this call, each dict will also
        contain a `"speaker"` key.

        For word-level assignment, each dict may optionally contain a
        `"words"` key (list of dicts with `"start"` and `"end"`), and
        each word dict will also receive a `"speaker"` key.

        Parameters:
            diarization (Annotation): pyannote diarization result.
            segments (list[dict]): Transcript segments to label.

        Returns:
            list[dict]: The same list, now with `"speaker"` keys populated.
        """
        for seg in segments:
            seg["speaker"] = DiarizationPipeline._find_dominant_speaker(
                diarization, seg["start"], seg["end"]
            )

            # Optionally assign speakers at word level too
            for word in seg.get("words", []):
                word["speaker"] = DiarizationPipeline._find_dominant_speaker(
                    diarization, word["start"], word["end"]
                )

        return segments

    @staticmethod
    def _find_dominant_speaker(
            diarization: Annotation,
            start: float,
            end: float,
    ) -> str:
        """
        Find the speaker with the most temporal overlap in a given interval.

        Parameters:
            diarization (Annotation): pyannote diarization result.
            start (float): Interval start time in seconds.
            end (float): Interval end time in seconds.

        Returns:
            str: Speaker label (e.g. "SPEAKER_00"), or "" if no overlap found.
        """
        target = Segment(start, end)
        best_speaker = ""
        best_overlap = 0.0

        for turn, _, speaker in diarization.itertracks(yield_label=True):
            overlap = target & turn
            if overlap is not None:
                overlap_duration = overlap.end - overlap.start
                if overlap_duration > best_overlap:
                    best_overlap = overlap_duration
                    best_speaker = speaker

        return best_speaker
