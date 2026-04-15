import logging
from typing import List, Optional, Any

import voyageai

from src.core.settings import Settings
from src.reranker.domain import DomainRerankingResponse, DomainRerankingResult

logger = logging.getLogger(__name__)


class RerankerError(Exception):
    """Custom exception raised when the upstream provider fails."""

    pass


class RerankerEngine:
    """
    Core business logic for reranking documents.
    Currently, wraps Voyage AI, but exposes a generic domain interface.
    """

    DEFAULT_TRUNCATE = True

    def __init__(self, settings: Settings):
        api_key = (
            settings.embeddings.api_key if hasattr(settings, "embeddings") else None
        )

        if not api_key:
            logger.warning("No Voyage API key found in settings. Engine may fail.")

        self.client = voyageai.Client(api_key=api_key)

    def rerank(
        self,
        model: str,
        query: str,
        documents: List[str],
        top_k: Optional[int] = None,
        truncate: Optional[bool] = None,
    ) -> DomainRerankingResponse:
        """
        Takes a query and raw document text inputs, returning domain reranking objects.

        Args:
            model: The model identifier to use for reranking.
            query: The query to rerank documents against.
            documents: The list of document strings to rerank.
            top_k: Optional maximum number of results to return.
            truncate: Whether to truncate documents that exceed the model's
                context length. When None (i.e. the caller did not explicitly
                set a value), defaults to True so that long documents are
                silently truncated rather than rejected.
        """
        effective_truncate = truncate if truncate is not None else self.DEFAULT_TRUNCATE

        logger.debug(f"Reranking {len(documents)} documents using {model}")

        try:
            kwargs: dict[str, Any] = {
                "query": query,
                "documents": documents,
                "model": model,
                "truncation": effective_truncate,
            }
            if top_k is not None:
                kwargs["top_k"] = top_k

            result = self.client.rerank(**kwargs)

        except Exception as e:
            logger.exception("Voyage AI reranking request failed.")
            raise RerankerError(f"Failed to fetch reranking results: {str(e)}") from e

        domain_results: List[DomainRerankingResult] = []

        for res in result.results:
            doc_index = int(getattr(res, "index"))

            domain_results.append(
                DomainRerankingResult(
                    relevance_score=float(res.relevance_score),
                    index=doc_index,
                    document=documents[doc_index],
                )
            )

        tokens_used: int = getattr(result, "total_tokens", 0)

        return DomainRerankingResponse(
            model=model,
            results=domain_results,
            total_tokens=tokens_used,
        )
