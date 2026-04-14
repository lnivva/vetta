from dataclasses import dataclass
from typing import List


@dataclass
class DomainEmbedding:
    """Represents a single vector embedding."""

    vector: List[float]
    index: int

    def __post_init__(self):
        """Runtime validation to ensure the parsed API payload is strictly correct."""
        if not isinstance(self.vector, list):
            raise TypeError(
                f"Expected 'vector' to be a list, got {type(self.vector).__name__}"
            )

        if not isinstance(self.index, int):
            raise TypeError(
                f"Expected 'index' to be an int, got {type(self.index).__name__}"
            )

        if any(
            not isinstance(v, (float, int)) or isinstance(v, bool) for v in self.vector
        ):
            raise TypeError("Vector must contain only numeric (non-bool) values.")


@dataclass
class DomainEmbeddingResponse:
    """The full result of an embedding request."""

    model: str
    embeddings: List[DomainEmbedding]
    prompt_tokens: int
    total_tokens: int

    def __post_init__(self):
        if not isinstance(self.embeddings, list):
            raise TypeError(
                "Expected 'embeddings' to be a list of DomainEmbedding objects."
            )
        if not all(isinstance(e, DomainEmbedding) for e in self.embeddings):
            raise TypeError(
                "All items in 'embeddings' must be DomainEmbedding instances."
            )
