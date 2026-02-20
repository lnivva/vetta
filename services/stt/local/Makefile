PROTO_DIR = ../../../proto
PROTO_OUT = .

PROTOS := $(shell find $(PROTO_DIR) -name '*.proto')

.PHONY: proto setup test test-unit test-integration clean format

proto:
	uv run python -m grpc_tools.protoc \
	   -I $(PROTO_DIR) \
	   --python_out=$(PROTO_OUT) \
	   --pyi_out=$(PROTO_OUT) \
	   --grpc_python_out=$(PROTO_OUT) \
	   $(PROTOS)

	find $(PROTO_OUT) -name "*_pb2.py" -execdir touch __init__.py \;

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

clean:
	# Dynamically remove all generated proto files anywhere in the project
	find . -name "*_pb2.py" -delete
	find . -name "*_pb2_grpc.py" -delete
	find . -name "*_pb2.pyi" -delete
	rm -rf __pycache__ .pytest_cache