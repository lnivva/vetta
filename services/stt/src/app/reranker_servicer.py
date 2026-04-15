import logging

import grpc

from src.generated.reranker import reranker_pb2_grpc, reranker_pb2
from src.core.settings import Settings
from src.reranker.engine import RerankerEngine, RerankerError

logger = logging.getLogger(__name__)


class RerankerServicer(reranker_pb2_grpc.RerankerServiceServicer):
    """
    gRPC Adapter for document reranking.
    Unpacks requests, interacts with the Voyage AI (or generic) engine,
    and handles upstream API errors or invalid arguments.
    """

    def __init__(self, settings: Settings):
        self._engine = RerankerEngine(settings)

    def Rerank(self, request, context):
        """
        Unpack the gRPC request and fetch reranked documents from the domain engine.
        """
        model = request.model
        query = request.query
        documents = list(request.documents)

        if not model or not model.strip():
            context.abort(
                grpc.StatusCode.INVALID_ARGUMENT, "Model identifier cannot be empty."
            )
            return None

        if not query or not query.strip():
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, "Query cannot be empty.")
            return None

        if not documents or any(not doc.strip() for doc in documents):
            context.abort(
                grpc.StatusCode.INVALID_ARGUMENT,
                "Documents list cannot be empty or contain blank entries.",
            )
            return None

        top_k = request.top_k if request.HasField("top_k") else None
        truncate = request.truncate if request.HasField("truncate") else None

        if top_k is not None and top_k <= 0:
            context.abort(
                grpc.StatusCode.INVALID_ARGUMENT, "top_k must be greater than 0."
            )

        try:
            domain_response = self._engine.rerank(
                model=model,
                query=query,
                documents=documents,
                top_k=top_k,
                truncate=truncate,
            )

            return self._map_to_proto(domain_response)

        except RerankerError:
            logger.exception("Reranking generation failed")
            context.abort(
                grpc.StatusCode.INTERNAL, "Failed to rerank documents via provider."
            )
        except Exception:
            logger.exception("Unexpected error in RerankerEngine")
            context.abort(grpc.StatusCode.INTERNAL, "An unexpected error occurred.")

    @staticmethod
    def _map_to_proto(domain_response) -> reranker_pb2.RerankResponse:
        """
        Maps the domain DomainRerankingResponse to the Protobuf message.
        """
        return reranker_pb2.RerankResponse(
            model=domain_response.model,
            results=[
                reranker_pb2.RerankingResult(
                    relevance_score=res.relevance_score,
                    index=res.index,
                    **({"document": res.document} if res.document is not None else {}),
                )
                for res in domain_response.results
            ],
            usage=reranker_pb2.Usage(
                total_tokens=domain_response.total_tokens,
            ),
        )
