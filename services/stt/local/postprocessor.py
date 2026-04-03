"""
Transcript post-processing pipeline for Whisper ASR output.

This module cleans and normalizes raw Whisper speech-to-text output into
high-quality, search- and embedding-ready text. It is designed for use
in production transcription pipelines (earnings calls, meetings,
interviews) and integrates cleanly with diarized, streaming STT systems.

Design goals
------------
- Improve readability (punctuation, casing)
- Normalize common ASR errors (entities, acronyms, financial terms)
- Preserve speaker boundaries and timing
- Produce stable text suitable for search and embeddings
- Fail gracefully when optional ML dependencies are unavailable

Processing order
----------------
1. Whitespace normalization
2. Entity correction (dictionary-based)
3. Speaker-aware sentence stitching (optional)
4. Neural punctuation restoration (optional)
5. Punctuation spacing cleanup
6. Truecasing
7. Final whitespace normalization

Why order matters
-----------------
Entity correction runs before punctuation restoration because punctuation
models perform better when tokens are correctly normalized. Segment
stitching precedes punctuation to give the model full sentence context
without violating speaker boundaries.

Threading and performance
-------------------------
- Instances are NOT thread-safe
- Create one instance per worker/thread
- Punctuation model is lazily loaded and cached per instance
- GPU acceleration is automatic when CUDA is available

Typical usage
-------------
Text-level:

    pp = TranscriptPostProcessor()
    pp.process_text("good day welcome to mongo db q four earnings call")
    # -> "Good day. Welcome to MongoDB Q4 earnings call."

Segment-level (recommended for diarized STT):

    pp.process_segments(segments, preserve_raw=True, stitch=True)
"""

from __future__ import annotations

import logging
import re
from dataclasses import dataclass, field
from typing import Any, Optional

logger = logging.getLogger(__name__)

_RAW_CORRECTIONS: dict[str, str] = {
    # ── Companies ─────────────────────────────────────────────
    "mongos db's": "MongoDB's",
    "mongos dbs": "MongoDB's",
    "mongos db": "MongoDB",
    "mongo db's": "MongoDB's",
    "mongo db": "MongoDB",
    "mongo d b": "MongoDB",
    "meta platforms": "Meta Platforms",
    "snow flake": "Snowflake",
    "data bricks": "Databricks",
    "cloud flair": "Cloudflare",
    "cloud flare": "Cloudflare",
    "sales force": "Salesforce",
    "crowd strike": "CrowdStrike",
    "palo alto": "Palo Alto",
    "service now": "ServiceNow",
    "data dog": "Datadog",
    "hub spot": "HubSpot",
    "git hub": "GitHub",
    "open ai": "OpenAI",
    "chat gpt": "ChatGPT",
    "g p t": "GPT",
    "google cloud": "Google Cloud",
    "a w s": "AWS",
    # ── Financial terms ──────────────────────────────────────
    "non-gap": "non-GAAP",
    "non gap": "non-GAAP",
    "non gaap": "non-GAAP",
    "gaap": "GAAP",
    "e p s": "EPS",
    "ebitda": "EBITDA",
    "a r r": "ARR",
    "m r r": "MRR",
    "t a m": "TAM",
    "i p o": "IPO",
    "r o i": "ROI",
    "s a a s": "SaaS",
    "saas": "SaaS",
    "p a a s": "PaaS",
    "i a a s": "IaaS",
    "f c f": "FCF",
    "o p e x": "OpEx",
    "opex": "OpEx",
    "cap ex": "CapEx",
    "capex": "CapEx",
    "cagr": "CAGR",
    # ── Temporal ─────────────────────────────────────────────
    "year over year": "year-over-year",
    "year on year": "year-on-year",
    "quarter over quarter": "quarter-over-quarter",
    # ── Titles ───────────────────────────────────────────────
    "c e o": "CEO",
    "c f o": "CFO",
    "c t o": "CTO",
    "c o o": "COO",
    "c i o": "CIO",
    "v p": "VP",
    # ── Quarters ─────────────────────────────────────────────
    "q one": "Q1",
    "q two": "Q2",
    "q three": "Q3",
    "q four": "Q4",
    "q 1": "Q1",
    "q 2": "Q2",
    "q 3": "Q3",
    "q 4": "Q4",
}


def _compile_corrections(
        raw: dict[str, str],
) -> list[tuple[re.Pattern[str], str]]:
    """
    Compile correction rules into regex patterns sorted longest-first.

    Longest-first ordering prevents partial shadowing
    (e.g. "non gaap" before "gaap").
    """
    patterns: list[tuple[re.Pattern[str], str]] = []
    for wrong, correct in sorted(raw.items(), key=lambda kv: -len(kv[0])):
        pat = re.compile(rf"\b{re.escape(wrong)}\b", re.IGNORECASE)
        patterns.append((pat, correct))
    return patterns


_GLOBAL_CORRECTIONS = _compile_corrections(_RAW_CORRECTIONS)

_MULTI_SPACE_RE = re.compile(r"\s{2,}")
_SPACE_BEFORE_PUNCT_RE = re.compile(r"\s+([.!?,;:])")
_SENTENCE_START_RE = re.compile(r"(?<=[.!?]\s)([a-z])")

_PUNCTUATION_CHUNK_WORDS = 500
_PUNCTUATION_OVERLAP_WORDS = 50
_DEFAULT_PUNCTUATION_MODEL = "oliverguhr/fullstop-punctuation-multilang-large"

_SENTENCE_END_CHARS = {".", "!", "?"}
_MAX_STITCH_GAP_SECONDS = 1.0
_MAX_STITCH_WORDS = 200
_STITCH_OUTPUT_KEYS = frozenset(
    {"speaker_id", "start_time", "end_time", "text", "words"}
)


@dataclass
class _StitchBuffer:
    speaker_id: Optional[str]
    start_time: float
    end_time: float
    parts: list[str]
    word_count: int
    words: list[dict[str, Any]]
    extra: dict[str, Any]

    def append(self, text: str, end_time: float, words: list[dict[str, Any]]) -> None:
        self.parts.append(text)
        self.word_count += len(text.split())
        self.end_time = end_time
        self.words.extend(words)

    def text(self) -> str:
        return " ".join(self.parts)

    def to_segment(self) -> dict[str, Any]:
        """Emit a merged segment, preserving all extra metadata."""
        seg: dict[str, Any] = {**self.extra}
        seg.update(
            {
                "speaker_id": self.speaker_id,
                "start_time": self.start_time,
                "end_time": self.end_time,
                "text": self.text(),
                "words": self.words,
            }
        )
        return seg


@dataclass(frozen=True, slots=True)
class PostProcessorConfig:
    """
    Immutable configuration for TranscriptPostProcessor.
    """

    enable_punctuation: bool = True
    enable_entity_correction: bool = True
    enable_truecasing: bool = True
    punctuation_model: str = _DEFAULT_PUNCTUATION_MODEL
    extra_corrections: dict[str, str] = field(default_factory=dict)


def _as_float(value: Any, default: float) -> float:
    if isinstance(value, (int, float)):
        return float(value)
    return default


class TranscriptPostProcessor:
    """
    Clean raw Whisper output for downstream search and embeddings.

    ⚠ Not thread-safe. Create one instance per worker.
    """

    __slots__ = (
        "_config",
        "_punctuator",
        "_extra_patterns",
        "_punct_available",
    )

    def __init__(self, config: Optional[PostProcessorConfig] = None) -> None:
        self._config = config or PostProcessorConfig()
        self._punctuator: Optional[Any] = None
        self._punct_available: Optional[bool] = None
        merged_corrections = {**_RAW_CORRECTIONS, **self._config.extra_corrections}
        self._extra_patterns = _compile_corrections(merged_corrections)

    @staticmethod
    def stitch_segments(segments: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """
        Merge adjacent segments when they are clearly parts of the same sentence.

        Preserves nested word lists, all timing / speaker metadata, and any
        extra fields through the merge.
        """
        if not segments:
            return segments

        stitched: list[dict[str, Any]] = []
        buffer: Optional[_StitchBuffer] = None

        for seg in segments:
            text = seg.get("text", "").strip()
            if not text:
                continue

            speaker = seg.get("speaker_id") or seg.get("speaker")

            raw_start = seg.get("start_time", seg.get("start"))
            start = _as_float(raw_start, 0.0)

            raw_end = seg.get("end_time", seg.get("end"))
            end = _as_float(raw_end, start)

            seg_words = list(seg.get("words", []))

            extra = {k: v for k, v in seg.items() if k not in _STITCH_OUTPUT_KEYS}
            extra.pop("start", None)
            extra.pop("end", None)

            if buffer is None:
                buffer = _StitchBuffer(
                    speaker_id=speaker,
                    start_time=start,
                    end_time=end,
                    parts=[text],
                    word_count=len(text.split()),
                    words=list(seg_words),
                    extra=extra,
                )
                continue

            gap = start - buffer.end_time
            prev_text = buffer.parts[-1]
            prev_complete = prev_text[-1] in _SENTENCE_END_CHARS

            next_word_count = len(text.split())
            same_speaker = bool(speaker) and speaker == buffer.speaker_id
            if (
                    same_speaker
                    and gap <= _MAX_STITCH_GAP_SECONDS
                    and not prev_complete
                    and buffer.word_count + next_word_count <= _MAX_STITCH_WORDS
            ):
                buffer.append(text, end, seg_words)
            else:
                stitched.append(buffer.to_segment())
                buffer = _StitchBuffer(
                    speaker_id=speaker,
                    start_time=start,
                    end_time=end,
                    parts=[text],
                    word_count=len(text.split()),
                    words=list(seg_words),
                    extra=extra,
                )

        if buffer:
            stitched.append(buffer.to_segment())

        return stitched

    @staticmethod
    def normalize_whitespace(text: str) -> str:
        return _MULTI_SPACE_RE.sub(" ", text).strip()

    def correct_entities(self, text: str) -> str:
        for pattern, repl in self._extra_patterns:
            text = pattern.sub(repl, text)
        return text

    def _get_punctuator(self) -> Optional[Any]:
        if self._punct_available is False:
            return None
        if self._punctuator:
            return self._punctuator

        try:
            import torch
            from transformers import pipeline
        except ImportError:
            logger.warning("transformers/torch not installed; disabling punctuation", exc_info=True)
            self._punct_available = False
            return None

        try:
            device = 0 if torch.cuda.is_available() else -1
            self._punctuator = pipeline(
                task="token-classification",
                model=self._config.punctuation_model,
                device=device,
            )
        except Exception:
            logger.exception("Failed to load punctuation model")
            self._punct_available = False
            return None

        self._punct_available = True
        return self._punctuator

    def restore_punctuation(self, text: str) -> str:
        punctuator = self._get_punctuator()
        if not punctuator:
            return text

        words = text.split()
        if not words:
            return text

        try:
            results = punctuator(text)

            punc_map = {
                "0": "",
                ".": ".",
                ",": ",",
                "?": "?",
                "-": "-",
                ":": ":",
            }
            if isinstance(results, list):
                output_text = ""
                for entity in results:
                    word = entity.get("word", "")
                    word = word.replace(" ", " ")
                    label = entity.get("entity") or entity.get("entity_group")
                    output_text += word + punc_map.get(label, "")

                return output_text.strip()

        except Exception as e:
            logger.error(f"Punctuation restoration failed: {e}")
            return text

        return text

    @staticmethod
    def clean_punctuation_spacing(text: str) -> str:
        return _SPACE_BEFORE_PUNCT_RE.sub(r"\1", text)

    @staticmethod
    def truecase(text: str) -> str:
        if not text:
            return text
        text = text[0].upper() + text[1:]
        return _SENTENCE_START_RE.sub(lambda m: m.group(1).upper(), text)

    def process_text(self, text: str) -> str:
        if not text.strip():
            return text

        text = self.normalize_whitespace(text)

        if self._config.enable_entity_correction:
            text = self.correct_entities(text)

        if self._config.enable_punctuation:
            text = self.restore_punctuation(text)
            text = self.clean_punctuation_spacing(text)

        if self._config.enable_truecasing:
            text = self.truecase(text)

        return self.normalize_whitespace(text)

    def process_segments(
            self,
            segments: list[dict[str, Any]],
            *,
            preserve_raw: bool = True,
            stitch: bool = True,
    ) -> list[dict[str, Any]]:
        """
        Process a list of transcript segments in place.

        - Optionally stitches segments (speaker-aware)
        - Preserves raw text if requested
        - Post-processes word-level text alongside segment text
        - Mutates the original list object
        """

        if not segments:
            return segments

        # ── Stitching (in-place semantics) ───────────────────────
        if stitch:
            stitched = self.stitch_segments(segments)
            segments.clear()
            segments.extend(stitched)

        # ── Text processing ─────────────────────────────────────
        for seg in segments:
            original = seg.get("text", "")
            if preserve_raw:
                seg.setdefault("text_raw", original)
            seg["text"] = self.process_text(original)

            # ── Word-level text processing ──────────────────────────
            for word in seg.get("words", []):
                word_text = word.get("text", "")
                if preserve_raw:
                    word.setdefault("text_raw", word_text)
                normalized_word = self.normalize_whitespace(word_text)
                if self._config.enable_entity_correction:
                    normalized_word = self.correct_entities(normalized_word)
                word["text"] = normalized_word

        return segments
