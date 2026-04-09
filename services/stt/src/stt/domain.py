from dataclasses import dataclass
from typing import List


@dataclass
class WordSegment:
    start_time: float
    end_time: float
    text: str
    confidence: float
    speaker_id: str


@dataclass
class TranscriptChunkResult:
    start_time: float
    end_time: float
    text: str
    speaker_id: str
    confidence: float
    words: List[WordSegment]
