# Show available commands
default:
    @just --list

proto_dir := "proto"

# ── STT service (Python) ────────────────────────────────────────────

stt_dir := "services/stt"
stt_venv := stt_dir / ".venv"
stt_generated_dir := stt_dir / "src/generated"
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
    @echo "Cleaning generated protobuf files in {{ stt_generated_dir }}..."
    rm -rf {{ stt_generated_dir }}

# Generate protobuf/gRPC Python code for stt
stt-proto: stt-clean-proto stt-venv
    #!/usr/bin/env bash
    set -euo pipefail

    # 1. Setup absolute paths to completely avoid relative path hell
    export ROOT="$PWD"
    export OUT_DIR="$ROOT/{{ stt_generated_dir }}"
    export PROTO_DIR="$ROOT/{{ proto_dir }}"
    export PYTHON_BIN="$ROOT/{{ stt_venv }}/bin/python"

    echo "Generating protobuf/gRPC code into {{ stt_generated_dir }}..."

    # 2. Create the target directory and base __init__.py
    mkdir -p "$OUT_DIR"
    touch "$OUT_DIR/__init__.py"

    # 3. Enter the proto directory.
    cd "$PROTO_DIR"
    protos=$(find . -name '*.proto')

    # 4. Run the generator using the venv's python
    "$PYTHON_BIN" -m grpc_tools.protoc \
        -I . \
        --python_out="$OUT_DIR" \
        --pyi_out="$OUT_DIR" \
        --grpc_python_out="$OUT_DIR" \
        $protos

    echo "Ensuring Python packages..."
    find "$OUT_DIR" -type d -exec touch {}/__init__.py \;

    # 5. Patch the generated gRPC imports for the `src.generated` namespace
    echo "Patching gRPC imports..."
    "$PYTHON_BIN" - << 'EOF'
    import glob, re, os

    out_dir = os.environ['OUT_DIR']
    pattern = re.compile(r'^from (\w+) import', re.MULTILINE)

    for p in glob.glob(os.path.join(out_dir, '**', '*_pb2_grpc.py'), recursive=True):
        with open(p, 'r') as f:
            code = f.read()

        # Safely rewrites 'from speech import' -> 'from src.generated.speech import'
        # and 'from embeddings import' -> 'from src.generated.embeddings import'
        code = pattern.sub(r'from src.generated.\1 import', code)

        with open(p, 'w') as f:
            f.write(code)
    EOF

    echo "Checking generated files..."
    test -n "$(find "$OUT_DIR" -name '*_pb2.py' -print -quit)" \
        || (echo "No proto files generated!" && exit 1)

    echo "Successfully generated protobufs!"

# Sync stt venv and generate protobuf code
stt-setup: stt-venv stt-proto

# Start the stt service with CUDA 12 isolation
stt-run: stt-setup
    @echo "Starting STT & Embedding services with CUDA 12 isolation..."
    cd {{ stt_dir }} && LD_LIBRARY_PATH={{ nvidia_libs }}:${LD_LIBRARY_PATH:-} \
        uv run python -m src.app.main --config config.toml

# Format stt Python code with ruff
stt-format: stt-venv
    cd {{ stt_dir }} && uv run ruff format .

# Run all stt tests
stt-test: stt-setup
    @echo "Running all stt tests..."
    cd {{ stt_dir }} && LD_LIBRARY_PATH={{ nvidia_libs }}:${LD_LIBRARY_PATH:-} \
        uv run pytest -v || (ret=$?; [ $ret -eq 5 ] && echo "No tests found, skipping gracefully." && exit 0 || exit $ret)

# Remove all stt build artifacts (proto + venv)
stt-clean: stt-clean-proto stt-clean-venv

# Check stt formatting (CI)
stt-format-check: stt-venv
    cd {{ stt_dir }} && uv run ruff format --check .

# Lint stt Python code with ruff
stt-lint: stt-venv
    cd {{ stt_dir }} && uv run ruff check .

# Type-check stt Python code with mypy
stt-typecheck: stt-venv
    cd {{ stt_dir }} && uv run mypy .

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

# ── Rust workspace ──────────────────────────────────────────────────

rust_dir := "rust"

# Check Rust formatting (CI)
rust-format-check:
    cd {{ rust_dir }} && cargo fmt --all -- --check

# Format Rust code
rust-format:
    cd {{ rust_dir }} && cargo fmt --all

# Lint Rust code with Clippy
rust-lint:
    cd {{ rust_dir }} && cargo clippy --workspace --all-targets --all-features -- -D warnings

# Build the Rust workspace
rust-build:
    cd {{ rust_dir }} && cargo build --workspace --verbose

# Run all Rust tests
rust-test:
    cd {{ rust_dir }} && cargo test --workspace --verbose

# Build in release mode
rust-build-release:
    cd {{ rust_dir }} && cargo build --workspace --release --verbose

# Clean Rust build artifacts
rust-clean:
    cd {{ rust_dir }} && cargo clean
