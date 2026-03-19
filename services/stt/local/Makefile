PROTO_DIR = ../../../proto
PROTO_OUT = .

PROTOS := $(shell find $(PROTO_DIR) -name '*.proto')

.PHONY: all clean proto setup test test-unit test-integration clean-proto rebuild-proto format proto-check

all: proto

clean: clean-proto

proto: clean-proto
	@echo "Generating protobuf/gRPC code..."
	uv run python -m grpc_tools.protoc \
	   -I $(PROTO_DIR) \
	   --python_out=$(PROTO_OUT) \
	   --pyi_out=$(PROTO_OUT) \
	   --grpc_python_out=$(PROTO_OUT) \
	   $(PROTOS)

	@echo "Ensuring Python packages..."
	find $(PROTO_OUT) -name "*_pb2.py" -execdir touch __init__.py \;
	@$(MAKE) proto-check

clean-proto:
	@echo "Cleaning generated protobuf files..."
	find . -name "*_pb2.py" -delete
	find . -name "*_pb2_grpc.py" -delete
	find . -name "*_pb2.pyi" -delete
	find . -name "__pycache__" -type d -exec rm -rf {} +

rebuild-proto: clean-proto proto
	@echo "Protobuf fully rebuilt."

proto-check:
	@echo "Checking generated files..."
	@test -n "$$(find . -name '*_pb2.py')" || (echo "No proto files generated!" && exit 1)

setup: proto
	uv sync

format:
	uv run ruff format .

test:
	uv run pytest -v

test-unit:
	uv run pytest tests/test_settings.py -v

test-integration:
	uv run pytest tests/test_integration.py -v