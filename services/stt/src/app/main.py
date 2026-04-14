"""
Entry point for the AI gRPC Application.

This script configures structured logging, initializes the gRPC server,
registers the STT and Embedding services, and handles the service lifecycle.
"""

import argparse
import logging
import os
import signal
from concurrent import futures
from pathlib import Path

import grpc
from pythonjsonlogger.json import JsonFormatter

from src.generated.speech import speech_pb2_grpc
from src.generated.embeddings import embeddings_pb2_grpc
from src.app.embed_servicer import EmbeddingServicer
from src.app.stt_servicer import SpeechToTextServicer
from src.core.settings import load_settings

logger = logging.getLogger(__name__)


def setup_logging():
    """Configure the root logger to emit structured JSON logs."""
    root_logger = logging.getLogger()
    root_logger.setLevel(logging.INFO)

    if not root_logger.handlers:
        log_handler = logging.StreamHandler()
        formatter = JsonFormatter("%(asctime)s %(levelname)s %(name)s %(message)s")
        log_handler.setFormatter(formatter)
        root_logger.addHandler(log_handler)


def serve(config_path: str):
    setup_logging()
    settings = load_settings(config_path)

    # 1. Initialize the core gRPC server
    server = grpc.server(
        futures.ThreadPoolExecutor(max_workers=settings.concurrency.max_workers)
    )

    # 2. Register Services
    speech_pb2_grpc.add_SpeechToTextServicer_to_server(
        SpeechToTextServicer(settings), server
    )
    embeddings_pb2_grpc.add_EmbeddingServiceServicer_to_server(
        EmbeddingServicer(settings), server
    )

    # 3. Network Binding Logic
    if settings.service.is_unix_socket:
        socket_path = settings.service.socket_path
        assert socket_path is not None, "Socket path must be provided for Unix sockets"

        # Ensure the parent directory exists
        os.makedirs(os.path.dirname(socket_path), exist_ok=True)

        # Clean up stale socket file if it crashed previously
        if os.path.exists(socket_path):
            os.unlink(socket_path)

        # gRPC explicitly requires the 'unix://' scheme for UDS
        bind_address = f"unix://{socket_path}"
        server.add_insecure_port(bind_address)
    else:
        # Standard TCP binding (e.g., "0.0.0.0:50051")
        bind_address = settings.service.address
        server.add_insecure_port(bind_address)

    # 4. Start Server
    server.start()

    # Apply tight permissions to the UDS *after* server.start() creates the file
    if settings.service.is_unix_socket:
        os.chmod(settings.service.socket_path, 0o600)

    logger.info("Service started", extra={"address": bind_address})

    # 5. Graceful Shutdown & Cleanup Handler
    def handle_shutdown(signum, frame):
        logger.info(f"Received signal {signum}, initiating graceful shutdown...")
        # Allow 10 seconds for active RPCs to finish
        stop_event = server.stop(grace=10)
        stop_event.wait()

        # Clean up the socket file gracefully on exit
        if settings.service.is_unix_socket and os.path.exists(
            settings.service.socket_path
        ):
            os.unlink(settings.service.socket_path)

        logger.info("Shutdown complete.")

    signal.signal(signal.SIGTERM, handle_shutdown)
    signal.signal(signal.SIGINT, handle_shutdown)

    # 6. Block main thread
    server.wait_for_termination()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Run the AI gRPC services.")
    parser.add_argument(
        "--config",
        default=str(Path(__file__).parent.parent.parent / "config.toml"),
        help="Path to config.toml",
    )
    args = parser.parse_args()
    serve(args.config)
