import logging

import grpc

from src.generated.embeddings import embeddings_pb2_grpc, embeddings_pb2
from src.core.settings import Settings
from src.embeddings.engine import EmbeddingsEngine, EmbeddingsError

logger = logging.getLogger(__name__)


class EmbeddingServicer(embeddings_pb2_grpc.EmbeddingServiceServicer):
    """
    gRPC Adapter for text embeddings.
    Unpacks requests, interacts with the Voyage AI (or generic) engine,
    and handles rate-limit or network errors.
    """

    def __init__(self, settings: Settings):
        self._engine = EmbeddingsEngine(settings)

    def CreateEmbeddings(self, request, context):
        """
        Unpack the gRPC request and fetch vector embeddings from the domain engine.
        """
        model = request.model
        inputs = list(request.inputs)
        truncate = request.truncate

        input_type = request.input_type if request.HasField("input_type") else None
        output_dim = (
            request.output_dimension if request.HasField("output_dimension") else None
        )

        if not inputs:
            context.abort(
                grpc.StatusCode.INVALID_ARGUMENT, "Inputs list cannot be empty."
            )
            return None

        if not model:
            context.abort(
                grpc.StatusCode.INVALID_ARGUMENT, "Model identifier cannot be empty."
            )
            return None

        try:
            domain_response = self._engine.embed(
                model=model,
                inputs=inputs,
                input_type=input_type,
                truncate=truncate,
                output_dimension=output_dim,
            )

            # 4. Pack and Return Response
            return self._map_to_proto(domain_response)

        except ValueError as exc:
            context.abort(grpc.StatusCode.INVALID_ARGUMENT, str(exc))
        except EmbeddingsError:
            logger.exception("Embedding generation failed")
            context.abort(
                grpc.StatusCode.INTERNAL, "Failed to generate embeddings via provider."
            )
        except Exception:
            logger.exception("Unexpected error in EmbeddingsEngine")
            context.abort(grpc.StatusCode.INTERNAL, "An unexpected error occurred.")

    @staticmethod
    def _map_to_proto(domain_response) -> embeddings_pb2.EmbeddingResponse:
        """
        Maps the domain DomainEmbeddingResponse to the Protobuf message.
        """
        return embeddings_pb2.EmbeddingResponse(
            model=domain_response.model,
            data=[
                embeddings_pb2.Embedding(vector=emb.vector, index=emb.index)
                for emb in domain_response.embeddings
            ],
            usage=embeddings_pb2.Usage(
                prompt_tokens=domain_response.prompt_tokens,
                total_tokens=domain_response.total_tokens,
            ),
        )
