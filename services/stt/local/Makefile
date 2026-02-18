PROTO_DIR = ../../../proto
PROTO_OUT  = .

proto:
	uv run python -m grpc_tools.protoc \
		-I $(PROTO_DIR) \
		--python_out=$(PROTO_OUT) \
		--grpc_python_out=$(PROTO_OUT) \
		speech.proto

setup:
	uv sync

test:
	uv run pytest -v

test-unit:
	uv run pytest tests/test_settings.py -v

test-integration:
	uv run pytest tests/test_integration.py -v