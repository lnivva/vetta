PROTO_DIR = ../../../proto
PROTO_OUT = .

PROTOS := $(shell find $(PROTO_DIR) -name '*.proto')
VENV_DIR = .venv
VENV_SENTINEL = $(VENV_DIR)/.sentinel

.PHONY: all proto setup test test-unit test-integration clean-proto rebuild-proto format proto-check venv clean-venv fresh-venv

all: proto

# ── Normal setup (fast, incremental) ────────────────────────────────
$(VENV_SENTINEL): pyproject.toml uv.lock
	@echo "Syncing venv with uv..."
	uv sync
	@touch $(VENV_SENTINEL)

venv: $(VENV_SENTINEL)

# ── Nuclear option (full rebuild) ───────────────────────────────────
clean-venv:
	@echo "Removing venv entirely..."
	rm -rf $(VENV_DIR)

fresh-venv: clean-venv $(VENV_SENTINEL)
	@echo "Fresh venv created."

clean: clean-proto clean-venv

# ── Protobuf ─────────────────────────────────────────────────────────
proto: clean-proto $(VENV_SENTINEL)
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

# ── Dev commands ─────────────────────────────────────────────────────
setup: $(VENV_SENTINEL) proto

format: $(VENV_SENTINEL)
	uv run ruff format .

test: $(VENV_SENTINEL)
	uv run pytest -v

test-unit: $(VENV_SENTINEL)
	uv run pytest tests/test_settings.py -v

test-integration: $(VENV_SENTINEL)
	uv run pytest tests/test_integration.py -v
