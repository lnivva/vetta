# Decision Log

Architectural decisions recorded here with their context and rationale.
Format: what was decided, why, and what was rejected.

---

## ADR-001: gRPC over Unix Domain Socket for STT IPC

**Decision:** Use gRPC with server-side streaming over a Unix domain socket for
communication between the Rust CLI and the Python STT service.

**Context:** We need a way for a Rust process to invoke a Python process that
holds a large ML model in memory, and receive results back as they are produced
over a ~2-hour audio file.

**Rejected alternatives:**

- **FastAPI/REST** — HTTP overhead, no native streaming, binary audio serialisation
  is awkward, port binding creates security surface
- **stdin/stdout subprocess** — No structured schema, brittle for binary data,
  cannot multiplex concurrent requests, hard to version
- **Shared memory** — Complex, unsafe across language boundaries, hard to version

**Why gRPC won:**
The proto contract is the strategy interface. `LocalSttStrategy` and any future
`CloudSttStrategy` are both gRPC clients — the Rust code is structurally identical
for both, only the transport (Unix socket vs TLS) differs.

**Why Unix Domain Socket over TCP:**
Security — no port exposed, access controlled entirely by file permissions (`chmod 0600`).
Performance — kernel-mediated, no loopback network stack.

---

## ADR-002: `faster-whisper` over `openai/whisper`

**Decision:** Use `faster-whisper` (CTranslate2 runtime) instead of the original
OpenAI Whisper (PyTorch runtime).

**Context:** We need to run `whisper-large-v3` locally. The model is ~3GB.
Speed and memory efficiency matter for a 2-hour audio file.

**Outcome:** 4x faster inference, 2x lower memory, no PyTorch dependency (~2GB saved),
no quality difference (same model weights, different runtime).

---

## ADR-003: Rust strategy trait, not enum dispatch

**Decision:** Define `SpeechToText` as a trait with `Box<dyn SpeechToText>` dispatch,
not an enum of concrete providers.

**Context:** We anticipate multiple STT providers (local Whisper, OpenAI, AssemblyAI).
We need a way to switch between them without modifying the pipeline.

**Why not enum:**
Adding a new provider would require touching the enum definition, the match arms,
and any downstream code that pattern-matches on it — violating open/closed principle.

**Why trait:**
Each provider is a self-contained module. Adding a new one is a new file and one
line in the CLI's strategy selection match. Everything else is untouched.

---

## ADR-004: `uv` for Python environment management

**Decision:** Use `uv` instead of `pyenv` + `venv` + `pip`.

**Context:** This is a monorepo. The Python service is one component alongside Rust.
We need Python version pinning, reproducible installs, and no global state that
could conflict with the Rust toolchain.

**Why `uv`:**
Single tool, single lockfile (`uv.lock`), reads `.python-version` automatically,
installs the pinned Python version if not present, creates the venv in the service
directory. `uv run <cmd>` uses the correct venv without activation.

**Rejected:** `pyenv` + `venv` + `pip` — three tools, manual activation step,
global pyenv state, `requirements.txt` has no lockfile semantics.

---

## ADR-005: File path in proto, not audio bytes

**Decision:** `TranscribeRequest.audio_path` carries an absolute path string,
not the raw audio bytes.

**Context:** A 2-hour earnings call MP3 is ~100MB. Both processes run on the same machine.

**Why path over bytes:**
Serialising 100MB per request over a Unix socket is wasteful and slow.
The filesystem is the most efficient shared medium between two local processes.

**Constraint this introduces:**
The Python service must have filesystem access to the path. In a future containerised
deployment, the audio file path must be on a shared mount. This is an acceptable
constraint for the local-first use case.

---

## ADR-006: Committed `uv.lock`, generated proto stubs not committed

**Decision:** Commit `uv.lock`. Do not commit `speech_pb2.py` / `speech_pb2_grpc.py`.

**`uv.lock` committed because:**
Reproducible installs across developer machines and CI. Without it, `uv sync` may
resolve different transitive dependency versions on different runs.

**Generated stubs not committed because:**
They are always reproducible from `proto/speech.proto` via `make proto`.
Committing them creates drift risk — the proto changes, someone forgets to regenerate,
the stubs are stale. CI regenerates them before running tests.
