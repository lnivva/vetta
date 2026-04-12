import logging
from typing import List, Optional, cast

import voyageai

from src.core.settings import Settings
from src.embeddings.domain import DomainEmbeddingResponse, DomainEmbedding

logger = logging.getLogger(__name__)


class EmbeddingsError(Exception):
    """Custom exception raised when the upstream provider fails."""

    pass


class EmbeddingsEngine:
    """
    Core business logic for generating text embeddings.
    Currently, wraps Voyage AI, but exposes a generic domain interface.
    """

    def __init__(self, settings: Settings):
        api_key = (
            settings.embeddings.api_key if hasattr(settings, "embeddings") else None
        )

        if not api_key:
            logger.warning("No embeddings API key found in settings. Engine may fail.")

        self.client = voyageai.Client(api_key=api_key)

    def embed(
        self,
        model: str,
        inputs: List[str],
        input_type: Optional[str] = None,
        truncate: bool = True,
        output_dimension: Optional[int] = None,
    ) -> DomainEmbeddingResponse:
        """
        Takes raw text inputs and returns domain embedding objects.
        """
        logger.debug(f"Generating embeddings for {len(inputs)} items using {model}")

        try:
            if output_dimension is not None:
                result = self.client.embed(
                    texts=inputs,
                    model=model,
                    input_type=input_type,
                    truncation=truncate,
                    output_dimension=output_dimension,
                )
            else:
                result = self.client.embed(
                    texts=inputs,
                    model=model,
                    input_type=input_type,
                    truncation=truncate,
                )

        except Exception as e:
            logger.exception("Voyage AI embedding request failed.")
            raise EmbeddingsError(f"Failed to fetch embeddings: {str(e)}") from e

        domain_embeddings: list[DomainEmbedding] = [
            DomainEmbedding(vector=cast(list[float], vec), index=i)
            for i, vec in enumerate(result.embeddings)
        ]

        tokens_used: int = getattr(result, "total_tokens", 0)

        return DomainEmbeddingResponse(
            model=model,
            embeddings=domain_embeddings,
            prompt_tokens=tokens_used,
            total_tokens=tokens_used,
        )
