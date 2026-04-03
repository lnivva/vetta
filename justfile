# Show available commands
default:
    @just --list

proto_dir := "proto"

# ── STT service (Python) ────────────────────────────────────────────

stt_dir := "services/stt"
stt_venv := stt_dir / ".venv"
stt_sentinel := stt_venv / ".sentinel"
nvidia_libs := join(justfile_directory(), stt_venv, "lib/python3.12/site-packages/nvidia/cublas/lib") + ":" + join(justfile_directory(), stt_venv, "lib/python3.12/site-packages/nvidia/cudnn/lib") + ":" + join(justfile_directory(), stt_venv, "lib/python3.12/site-packages/nvidia/cuda_nvrtc/lib") + ":" + join(justfile_directory(), stt_venv, "lib/python3.12/site-packages/nvidia/cuda_runtime/lib") + ":" + join(justfile_directory(), stt_venv, "lib/python3.12/site-packages/nvidia/npp/lib")

# Sync the stt virtualenv (only if sentinel is stale)
stt-venv:
    #!/usr/bin/env bash
    set -euo pipefail
    sentinel="{{ justfile_directory() }}/{{ stt_sentinel }}"
    pyproject="{{ justfile_directory() }}/{{ stt_dir }}/pyproject.toml"
    uvlock="{{ justfile_directory() }}/{{ stt_dir }}/uv.lock"
    if [[ "$pyproject" -nt "$sentinel" ]] || \
       [[ "$uvlock"    -nt "$sentinel" ]] || \
       [[ ! -f "$sentinel" ]]; then
        echo "Syncing stt venv with uv..."
        cd "{{ justfile_directory() }}/{{ stt_dir }}" && uv sync
        touch "$sentinel"
    else
        echo "stt venv is up to date."
    fi

# Remove stt virtualenv
stt-clean-venv:
    @echo "Removing stt venv entirely..."
    rm -rf {{ stt_venv }}

# Delete and recreate stt virtualenv from scratch
stt-fresh-venv: stt-clean-venv stt-venv
    @echo "Fresh stt venv created."

# Remove generated protobuf files from stt
stt-clean-proto:
    @echo "Cleaning generated protobuf files in {{ stt_dir }}..."
    find {{ stt_dir }} -type d -name ".venv" -prune -o -type f -name "*_pb2.py"      -exec rm -f {} +
    find {{ stt_dir }} -type d -name ".venv" -prune -o -type f -name "*_pb2_grpc.py"  -exec rm -f {} +
    find {{ stt_dir }} -type d -name ".venv" -prune -o -type f -name "*_pb2.pyi"      -exec rm -f {} +
    find {{ stt_dir }} -type d -name ".venv" -prune -o -type d -name "__pycache__"    -exec rm -rf {} +

# Generate protobuf/gRPC Python code for stt
stt-proto: stt-clean-proto stt-venv
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Generating protobuf/gRPC code for stt..."
    protos=$(find {{ proto_dir }} -name '*.proto')
    cd {{ stt_dir }} && uv run python -m grpc_tools.protoc \
        -I ../../{{ proto_dir }} \
        --python_out=. \
        --pyi_out=. \
        --grpc_python_out=. \
        $(echo "$protos" | sed 's|^|../../|')

    echo "Ensuring Python packages..."
    find . -type d -name ".venv" -prune -o -type f -name "*_pb2.py" -execdir touch __init__.py \;

    echo "Checking generated files..."
    test -n "$(find . -type d -name '.venv' -prune -o -type f -name '*_pb2.py' -print)" \
        || (echo "No proto files generated!" && exit 1)

# Clean and regenerate all stt protobuf files
stt-rebuild-proto: stt-clean-proto stt-proto
    @echo "Protobuf fully rebuilt."

# Sync stt venv and generate protobuf code
stt-setup: stt-venv stt-proto

# Start the stt service with CUDA 12 isolation
stt-run: stt-setup
    @echo "Starting stt service with CUDA 12 isolation..."
    cd {{ stt_dir }} && LD_LIBRARY_PATH={{ nvidia_libs }}:${LD_LIBRARY_PATH:-} \
        uv run python main.py --config config.toml

# Format stt Python code with ruff
stt-format: stt-venv
    cd {{ stt_dir }} && uv run ruff format .

# Run all stt tests
stt-test: stt-setup
    cd {{ stt_dir }} && LD_LIBRARY_PATH={{ nvidia_libs }}:${LD_LIBRARY_PATH:-} \
        uv run pytest -v

# Run stt unit tests only
stt-test-unit: stt-setup
    cd {{ stt_dir }} && LD_LIBRARY_PATH={{ nvidia_libs }}:${LD_LIBRARY_PATH:-} \
        uv run pytest tests/test_settings.py -v

# Run stt integration tests only
stt-test-integration: stt-setup
    cd {{ stt_dir }} && LD_LIBRARY_PATH={{ nvidia_libs }}:${LD_LIBRARY_PATH:-} \
        uv run pytest tests/test_integration.py -v

# Remove all stt build artifacts (proto + venv)
stt-clean: stt-clean-proto stt-clean-venv

# ── Docs (Rspress) ──────────────────────────────────────────────────

docs_dir := "docs"

# Install docs dependencies
docs-install:
    cd {{ docs_dir }} && npm install

# Start rspress dev server
docs-dev: docs-install
    cd {{ docs_dir }} && npx rspress dev

# Build static docs site
docs-build: docs-install
    cd {{ docs_dir }} && npx rspress build

# Preview the built site
docs-preview: docs-build
    cd {{ docs_dir }} && npx rspress preview

# Clean docs build artifacts
docs-clean:
    rm -rf {{ docs_dir }}/doc_build
    rm -rf {{ docs_dir }}/node_modules/.cache
