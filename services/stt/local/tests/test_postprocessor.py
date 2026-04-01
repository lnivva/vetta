"""
Tests for postprocessor.py

Run with:
    pytest tests/test_postprocessor.py -v
"""

from __future__ import annotations

import pytest

from postprocessor import (
    PostProcessorConfig,
    TranscriptPostProcessor,
    _compile_corrections,
)


# ============================================================================
# Fixtures
# ============================================================================


@pytest.fixture()
def processor_no_punct() -> TranscriptPostProcessor:
    """Processor with punctuation disabled (no ML dependency)."""
    config = PostProcessorConfig(
        enable_punctuation=False,
        enable_entity_correction=True,
        enable_truecasing=True,
    )
    return TranscriptPostProcessor(config)


@pytest.fixture()
def processor_entities_only() -> TranscriptPostProcessor:
    """Processor with only entity correction enabled."""
    config = PostProcessorConfig(
        enable_punctuation=False,
        enable_entity_correction=True,
        enable_truecasing=False,
    )
    return TranscriptPostProcessor(config)


@pytest.fixture()
def processor_all_disabled() -> TranscriptPostProcessor:
    """Processor with all optional transforms disabled."""
    config = PostProcessorConfig(
        enable_punctuation=False,
        enable_entity_correction=False,
        enable_truecasing=False,
    )
    return TranscriptPostProcessor(config)


# ============================================================================
# Whitespace normalization
# ============================================================================


class TestWhitespace:
    def test_collapses_multiple_spaces(self, processor_all_disabled):
        assert processor_all_disabled.process_text("hello    world") == "hello world"

    def test_strips_leading_trailing(self, processor_all_disabled):
        assert processor_all_disabled.process_text("  hello world  ") == "hello world"

    def test_empty_string_passthrough(self, processor_all_disabled):
        assert processor_all_disabled.process_text("") == ""

    def test_whitespace_only_preserved(self, processor_all_disabled):
        # Early-return preserves original whitespace-only input
        assert processor_all_disabled.process_text("   ") == "   "

    # ============================================================================


# Entity correction
# ============================================================================


class TestEntityCorrection:
    @pytest.mark.parametrize(
        "input_text, expected",
        [
            (
                "welcome to mongos db's earnings call",
                "welcome to MongoDB's earnings call",
            ),
            ("mongo db reported strong results", "MongoDB reported strong results"),
            ("mongo d b is growing", "MongoDB is growing"),
            ("our saas platform", "our SaaS platform"),
            ("non gaap operating margin", "non-GAAP operating margin"),
            ("non gap margins improved", "non-GAAP margins improved"),
            ("the c e o said", "the CEO said"),
            ("capex was lower", "CapEx was lower"),
            ("q four results", "Q4 results"),
            ("year over year growth", "year-over-year growth"),
            ("a r r exceeded one billion", "ARR exceeded one billion"),
            ("e p s came in at", "EPS came in at"),
            ("cloud flair reported", "Cloudflare reported"),
            ("data bricks partnership", "Databricks partnership"),
            ("service now integration", "ServiceNow integration"),
        ],
    )
    def test_global_corrections(self, processor_entities_only, input_text, expected):
        assert processor_entities_only.correct_entities(input_text) == expected

    def test_longer_pattern_takes_precedence(self, processor_entities_only):
        result = processor_entities_only.correct_entities(
            "both gaap and non gaap metrics improved"
        )
        assert result == "both GAAP and non-GAAP metrics improved"

    def test_case_insensitive(self, processor_entities_only):
        assert "MongoDB" in processor_entities_only.correct_entities(
            "Mongos DB is great"
        )

    def test_word_boundary_prevents_partial_match(self, processor_entities_only):
        result = processor_entities_only.correct_entities("agaap is not a word")
        assert result == "agaap is not a word"

    def test_extra_corrections(self):
        config = PostProcessorConfig(
            enable_punctuation=False,
            enable_entity_correction=True,
            enable_truecasing=False,
            extra_corrections={
                "jay pee morgan": "JPMorgan",
                "gold man sacks": "Goldman Sachs",
            },
        )
        pp = TranscriptPostProcessor(config)
        result = pp.correct_entities("jay pee morgan and gold man sacks analysts")
        assert result == "JPMorgan and Goldman Sachs analysts"

    def test_extra_corrections_override_global(self):
        config = PostProcessorConfig(
            enable_punctuation=False,
            enable_entity_correction=True,
            enable_truecasing=False,
            extra_corrections={"mongo db": "MongoDB, Inc."},
        )
        pp = TranscriptPostProcessor(config)
        assert pp.correct_entities("mongo db reported") == "MongoDB, Inc. reported"

    # ============================================================================


# Truecasing
# ============================================================================


class TestTruecasing:
    def test_capitalizes_first_character(self):
        assert TranscriptPostProcessor.truecase("hello world.") == "Hello world."

    def test_capitalizes_after_period(self):
        assert (
            TranscriptPostProcessor.truecase("revenue grew. margins improved.")
            == "Revenue grew. Margins improved."
        )

    def test_capitalizes_after_question_mark(self):
        assert (
            TranscriptPostProcessor.truecase("is that right? yes it is.")
            == "Is that right? Yes it is."
        )

    def test_capitalizes_after_exclamation(self):
        assert (
            TranscriptPostProcessor.truecase("great results! we exceeded targets.")
            == "Great results! We exceeded targets."
        )

    def test_preserves_existing_caps(self):
        text = "MongoDB reported strong ARR growth."
        assert TranscriptPostProcessor.truecase(text) == text

    def test_empty_string(self):
        assert TranscriptPostProcessor.truecase("") == ""

    # ============================================================================


# Punctuation spacing cleanup
# ============================================================================


class TestPunctuationSpacing:
    def test_removes_space_before_period(self):
        assert (
            TranscriptPostProcessor.clean_punctuation_spacing("revenue . We grew")
            == "revenue. We grew"
        )

    def test_removes_space_before_comma(self):
        assert (
            TranscriptPostProcessor.clean_punctuation_spacing("hello , world")
            == "hello, world"
        )

    def test_no_change_when_correct(self):
        text = "Revenue grew. Margins improved, as expected."
        assert TranscriptPostProcessor.clean_punctuation_spacing(text) == text

    # ============================================================================


# Full pipeline (no punctuation model)
# ============================================================================


class TestFullPipeline:
    def test_end_to_end_no_punctuation(self, processor_no_punct):
        raw = (
            "good day and thank you for standing by welcome to "
            "mongos db's fourth quarter earnings call the c e o "
            "discussed non gaap results"
        )

        result = processor_no_punct.process_text(raw)

        assert result.startswith("Good")
        assert "MongoDB's" in result
        assert "CEO" in result
        assert "non-GAAP" in result

    def test_passthrough_when_all_disabled(self, processor_all_disabled):
        raw = "hello   world  "
        assert processor_all_disabled.process_text(raw) == "hello world"

    # ============================================================================


# Segment stitching & batch processing
# ============================================================================


class TestProcessSegments:
    def test_preserves_raw_by_default(self, processor_no_punct):
        segments = [
            {
                "text": "mongos db reported",
                "start_time": 0.0,
                "end_time": 1.5,
                "speaker_id": "spk_0",
            },
            {
                "text": "saas growth was strong",
                "start_time": 1.5,
                "end_time": 3.0,
                "speaker_id": "spk_0",
            },
        ]

        processor_no_punct.process_segments(segments)

        # Stitching happened
        assert len(segments) == 1
        # Raw stitched text preserved
        assert segments[0]["text_raw"] == "mongos db reported saas growth was strong"
        # Fully processed stitched text
        assert segments[0]["text"] == "MongoDB reported SaaS growth was strong"

    def test_no_raw_when_disabled(self, processor_no_punct):
        segments = [
            {
                "text": "mongos db",
                "start_time": 0.0,
                "end_time": 1.0,
                "speaker_id": "spk_0",
            }
        ]

        processor_no_punct.process_segments(segments, preserve_raw=False)

        assert "text_raw" not in segments[0]
        assert segments[0]["text"] == "MongoDB"

    def test_empty_segments(self, processor_no_punct):
        assert processor_no_punct.process_segments([]) == []

    def test_missing_text_key(self, processor_no_punct):
        segments = [{"start_time": 0.0, "end_time": 1.0, "speaker_id": "spk_0"}]
        processor_no_punct.process_segments(segments)
        assert segments == []

    def test_mutation_in_place(self, processor_no_punct):
        segments = [
            {
                "text": "e p s beat",
                "start_time": 0.0,
                "end_time": 1.0,
                "speaker_id": "spk_0",
            }
        ]
        returned = processor_no_punct.process_segments(segments)
        assert returned is segments

    # ============================================================================


# Stitching logic
# ============================================================================


class TestStitching:
    def test_stitches_same_speaker_short_gap(self, processor_no_punct):
        segments = [
            {
                "text": "good morning",
                "start_time": 0.0,
                "end_time": 1.0,
                "speaker_id": "spk_0",
            },
            {
                "text": "welcome everyone",
                "start_time": 1.2,
                "end_time": 2.5,
                "speaker_id": "spk_0",
            },
        ]

        stitched = processor_no_punct.stitch_segments(segments)
        assert len(stitched) == 1
        assert stitched[0]["text"] == "good morning welcome everyone"

    def test_does_not_stitch_different_speakers(self, processor_no_punct):
        segments = [
            {
                "text": "good morning",
                "start_time": 0.0,
                "end_time": 1.0,
                "speaker_id": "spk_0",
            },
            {
                "text": "thank you",
                "start_time": 1.1,
                "end_time": 2.0,
                "speaker_id": "spk_1",
            },
        ]

        stitched = processor_no_punct.stitch_segments(segments)
        assert len(stitched) == 2

    def test_does_not_stitch_if_sentence_complete(self, processor_no_punct):
        segments = [
            {
                "text": "good morning.",
                "start_time": 0.0,
                "end_time": 1.0,
                "speaker_id": "spk_0",
            },
            {
                "text": "welcome everyone",
                "start_time": 1.1,
                "end_time": 2.5,
                "speaker_id": "spk_0",
            },
        ]

        stitched = processor_no_punct.stitch_segments(segments)
        assert len(stitched) == 2

    # ============================================================================


# Correction compilation helper
# ============================================================================


class TestCompileCorrections:
    def test_sorted_longest_first(self):
        raw = {"ab": "AB", "abc": "ABC", "a": "A"}
        patterns = _compile_corrections(raw)
        assert [p[1] for p in patterns] == ["ABC", "AB", "A"]

    def test_empty_dict(self):
        assert _compile_corrections({}) == []
