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

        if self.vector and not isinstance(self.vector[0], (float, int)):
            raise TypeError(
                f"Vector must contain numbers. Found: {type(self.vector[0]).__name__}"
            )


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
