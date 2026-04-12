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
    """
    Configure the root logger to emit structured JSON logs.
    """
    root_logger = logging.getLogger()
    root_logger.setLevel(logging.INFO)

    # Prevent adding multiple handlers if called multiple times
    if not root_logger.handlers:
        log_handler = logging.StreamHandler()
        formatter = JsonFormatter("%(asctime)s %(levelname)s %(name)s %(message)s")
        log_handler.setFormatter(formatter)
        root_logger.addHandler(log_handler)


def serve(config_path: str):
    setup_logging()

    settings = load_settings(config_path)
    address = settings.service.address
    socket_path = settings.service.socket_path

    assert socket_path is not None, "Socket path should not be None"

    if socket_path and os.path.exists(socket_path):
        os.unlink(socket_path)

    # Initialize the core gRPC server
    server = grpc.server(
        futures.ThreadPoolExecutor(max_workers=settings.concurrency.max_workers)
    )

    # ── Register Services ──────────────────────────────────────
    speech_pb2_grpc.add_SpeechToTextServicer_to_server(
        SpeechToTextServicer(settings), server
    )

    embeddings_pb2_grpc.add_EmbeddingServiceServicer_to_server(
        EmbeddingServicer(settings), server
    )
    # ───────────────────────────────────────────────────────────

    if settings.service.is_unix_socket:
        assert socket_path is not None  # Guaranteed by is_unix_socket
        server.add_insecure_port(address)
        server.start()
        os.chmod(socket_path, 0o600)
    else:
        # TODO: harden security (ssl/tls mtls)
        server.add_insecure_port(address)
        server.start()

    logger.info("Service started", extra={"address": address})

    def handle_shutdown(signum, frame):
        logger.info(f"Received signal {signum}, shutting down...")
        stop_event = server.stop(grace=10)
        stop_event.wait()
        logger.info("Shutdown complete.")

    signal.signal(signal.SIGTERM, handle_shutdown)
    signal.signal(signal.SIGINT, handle_shutdown)

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
