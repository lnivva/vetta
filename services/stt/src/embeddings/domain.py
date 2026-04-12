from dataclasses import dataclass
from typing import List


@dataclass
class DomainEmbedding:
    """Represents a single vector embedding."""

    vector: List[float]
    index: int


@dataclass
class DomainEmbeddingResponse:
    """The full result of an embedding request."""

    model: str
    embeddings: List[DomainEmbedding]
    prompt_tokens: int
    total_tokens: int
