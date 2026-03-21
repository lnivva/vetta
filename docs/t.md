# Reorganizing the Quick Start Guide

Great call — the current guide mixes platform-specific instructions, infrastructure choices, and service setup into one linear flow. That works for a single "happy path" but breaks down as soon as someone is on Linux, deploying to EC2, or already has MongoDB Atlas running.

Here's a proposed restructuring:

---

## Proposed Sidebar Structure

```javascript
"/guide/": [
  {
    text: "Getting Started",
    items: [
      { text: "Introduction", link: "/guide/introduction" },
      { text: "Quick Start", link: "/guide/quick-start" },
    ],
  },
  {
    text: "Installation",
    items: [
      { text: "Prerequisites", link: "/guide/installation/prerequisites" },
      { text: "macOS", link: "/guide/installation/macos" },
      { text: "Linux (Ubuntu/Debian)", link: "/guide/installation/linux" },
      { text: "Cloud (EC2 / Terraform)", link: "/guide/installation/cloud" },
    ],
  },
  {
    text: "MongoDB Setup",
    items: [
      { text: "Overview", link: "/guide/mongodb/overview" },
      { text: "Local (Atlas CLI)", link: "/guide/mongodb/local-atlas-cli" },
      { text: "Atlas Cloud", link: "/guide/mongodb/atlas-cloud" },
      { text: "Self-Hosted / Existing", link: "/guide/mongodb/self-hosted" },
    ],
  },
  {
    text: "Services",
    items: [
      { text: "STT Service", link: "/guide/services/stt" },
      { text: "Hugging Face Authentication", link: "/guide/services/hugging-face-auth" },
    ],
  },
  {
    text: "First Run",
    items: [
      { text: "Generate Test Audio", link: "/guide/first-run/test-audio" },
      { text: "Process an Earnings Call", link: "/guide/first-run/process" },
    ],
  },
],
```

---

## What Each Page Does

### `/guide/quick-start` — The 2-Minute Overview

This becomes a *short*, opinionated, "fastest path to a working system" page. It links out to the detailed pages but doesn't try to cover every scenario itself.

```markdown
# Quick Start

This gets you from zero to processing your first earnings call.
Pick your platform, connect MongoDB, start the STT service, and run the pipeline.

## 1. Install dependencies
> Follow the guide for your platform:
> **[macOS](/guide/installation/macos)** · **[Linux](/guide/installation/linux)** · **[Cloud (EC2)](/guide/installation/cloud)**

## 2. Clone and build
\`\`\`bash
git clone https://github.com/lnivva/vetta
cd vetta
cargo build
\`\`\`

## 3. Set up MongoDB
> Pick the option that fits:
> **[Local via Atlas CLI](/guide/mongodb/local-atlas-cli)** · **[Atlas Cloud](/guide/mongodb/atlas-cloud)** · **[Existing instance](/guide/mongodb/self-hosted)**

## 4. Configure Hugging Face access
> [Hugging Face Authentication →](/guide/services/hugging-face-auth)

## 5. Start the STT service
> [STT Service →](/guide/services/stt)

## 6. Process your first file
> [First Run →](/guide/first-run/process)
```

Short, scannable, no scrolling past macOS instructions you don't need.

---

### `/guide/installation/prerequisites` — Shared Dependencies

Platform-agnostic table of *what* you need (Rust, uv, protoc, ffmpeg) and *why*, without any install commands.

```markdown
# Prerequisites

| Tool    | Purpose                 | Required |
|---------|-------------------------|----------|
| Rust    | Core crate + CLI        | Yes      |
| uv      | Python env management   | Yes      |
| protoc  | Protobuf compilation    | Yes      |
| ffmpeg  | Audio format conversion | Yes      |

Select your platform for installation instructions:

::: tip
**[macOS →](/guide/installation/macos)** · **[Linux (Ubuntu/Debian) →](/guide/installation/linux)** · **[Cloud (EC2) →](/guide/installation/cloud)**
:::
```

---

### `/guide/installation/macos`

```markdown
# macOS Installation

## Homebrew (recommended)

\`\`\`bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Python env manager
curl -LsSf https://astral.sh/uv/install.sh | sh

# System dependencies
brew install protobuf ffmpeg
\`\`\`

## Verify

\`\`\`bash
rustc --version
uv --version
protoc --version
ffmpeg -version
\`\`\`
```

---

### `/guide/installation/linux`

```markdown
# Linux Installation (Ubuntu / Debian)

\`\`\`bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Python env manager
curl -LsSf https://astral.sh/uv/install.sh | sh

# System dependencies
sudo apt-get update
sudo apt-get install -y protobuf-compiler ffmpeg build-essential pkg-config libssl-dev
\`\`\`

## Verify

\`\`\`bash
rustc --version
uv --version
protoc --version
ffmpeg -version
\`\`\`
```

---

### `/guide/installation/cloud`

```markdown
# Cloud Deployment (EC2 / Terraform)

This page covers running Vetta on a remote Linux instance.
GPU instances (e.g., `g5.xlarge`) are recommended for faster Whisper inference.

## Manual EC2 Setup

1. Launch an Ubuntu 22.04+ instance (at least 8 GB RAM, 30 GB disk).
2. SSH in and follow the [Linux installation guide](/guide/installation/linux).
3. For GPU acceleration, install NVIDIA drivers and CUDA toolkit:

\`\`\`bash
sudo apt-get install -y nvidia-driver-535 nvidia-cuda-toolkit
\`\`\`

4. Set `device = "cuda"` in `services/stt/local/config.toml`.

## Terraform

A reference Terraform module is provided in `deploy/terraform/`:

\`\`\`bash
cd deploy/terraform
terraform init
terraform apply -var="instance_type=g5.xlarge"
\`\`\`

> This provisions an EC2 instance, security group, and outputs the SSH command.
> MongoDB connection is expected via [Atlas Cloud](/guide/mongodb/atlas-cloud) 
> or an existing deployment.

## Environment Variables

On remote instances, persist your variables in `/etc/environment` 
or use a secrets manager:

\`\`\`bash
echo 'MONGODB_URI="mongodb+srv://..."' | sudo tee -a /etc/environment
echo 'MONGODB_DATABASE="vetta"' | sudo tee -a /etc/environment
\`\`\`
```

---

### `/guide/mongodb/overview`

```markdown
# MongoDB Setup

Vetta stores transcripts, embeddings, and metadata in MongoDB. 
Two environment variables are required in every shell session:

| Variable           | Description              | Example                                            |
|--------------------|--------------------------|----------------------------------------------------|
| `MONGODB_URI`      | Connection string        | `mongodb://localhost:27017/?directConnection=true` |
| `MONGODB_DATABASE` | Database name            | `vetta`                                            |

Choose the setup that fits your situation:

| Option | Best for | Guide |
|--------|----------|-------|
| **Local (Atlas CLI)** | Development, offline work | [→ Local Setup](/guide/mongodb/local-atlas-cli) |
| **Atlas Cloud** | Production, team access, cloud deployments | [→ Atlas Cloud](/guide/mongodb/atlas-cloud) |
| **Self-Hosted / Existing** | You already have MongoDB running | [→ Self-Hosted](/guide/mongodb/self-hosted) |

::: warning
Both `MONGODB_URI` and `MONGODB_DATABASE` must be set before running any Vetta command.
Add them to `~/.bashrc`, `~/.zshrc`, or a `.env` file.
:::
```

---

### `/guide/mongodb/local-atlas-cli`

```markdown
# Local MongoDB with Atlas CLI

The Atlas CLI spins up a full-featured local deployment inside Docker — 
no cloud account needed.

## Prerequisites

A Docker-compatible container runtime:

::: code-group

\`\`\`bash [macOS (Colima)]
brew install mongodb-atlas-cli colima docker
colima start
\`\`\`

\`\`\`bash [macOS (Docker Desktop)]
brew install mongodb-atlas-cli
# Install Docker Desktop from https://www.docker.com/products/docker-desktop
\`\`\`

\`\`\`bash [Linux]
# Install Atlas CLI: https://www.mongodb.com/docs/atlas/cli/current/install-atlas-cli/
curl -fsSL https://www.mongodb.com/docs/atlas/cli/current/install-atlas-cli/ | bash
# Docker Engine must be running
sudo systemctl start docker
\`\`\`

:::

## Create the deployment

\`\`\`bash
atlas local setup vetta-local --port 27017 --bindIpAll
\`\`\`

On first run the CLI pulls container images. Once ready:

\`\`\`text
Deployment vetta-local created.
\`\`\`

## Export environment variables

\`\`\`bash
export MONGODB_URI="mongodb://localhost:27017/?directConnection=true"
export MONGODB_DATABASE="vetta"
\`\`\`

## Manage the deployment

\`\`\`bash
atlas local list                    # Check status
atlas local pause vetta-local       # Stop (data preserved)
atlas local start vetta-local       # Resume
atlas local delete vetta-local      # Remove (data lost)
\`\`\`
```

---

### `/guide/mongodb/atlas-cloud`

```markdown
# MongoDB Atlas (Cloud)

Best for production, team environments, and cloud-hosted Vetta instances.

## 1. Create a free cluster

1. Sign up at [mongodb.com/cloud/atlas](https://www.mongodb.com/cloud/atlas)
2. Create a free **M0** cluster (or higher for production workloads)
3. Under **Database Access**, create a database user
4. Under **Network Access**, add your IP address (or `0.0.0.0/0` for EC2 with security groups)

## 2. Get your connection string

Navigate to **Connect → Drivers** and copy the `mongodb+srv://` URI.

## 3. Export environment variables

\`\`\`bash
export MONGODB_URI="mongodb+srv://user:password@cluster0.xxxxx.mongodb.net/?retryWrites=true&w=majority"
export MONGODB_DATABASE="vetta"
\`\`\`

## Atlas Vector Search

If you plan to use Vetta's semantic search features, create a vector search index 
on the `segments` collection. See [Search & Retrieval](/technical/search-retrieval) 
for index definitions.
```

---

### `/guide/mongodb/self-hosted`

```markdown
# Self-Hosted / Existing MongoDB

If you already have a MongoDB instance running (self-managed, Docker, replica set, etc.), 
just export your connection details:

\`\`\`bash
export MONGODB_URI="your-connection-string"
export MONGODB_DATABASE="vetta"
\`\`\`

## Requirements

- MongoDB **6.0+** recommended
- For vector search: MongoDB 7.0+ with Atlas Search, or a compatible deployment
- The user in your connection string needs `readWrite` on the target database

## Verify connectivity

\`\`\`bash
mongosh "$MONGODB_URI" --eval "db.runCommand({ ping: 1 })"
\`\`\`
```

---

### `/guide/services/hugging-face-auth`

The existing Hugging Face token section, extracted as its own page. No changes to content needed.

---

### `/guide/services/stt`

The existing STT startup section, extracted as its own page. You could add a note about `device = "cuda"` for GPU instances.

---

### `/guide/first-run/test-audio`

```markdown
# Generate Test Audio

::: code-group

\`\`\`bash [macOS (say)]
say -v Samantha "Good morning everyone and welcome to the Q3 2024 earnings call. \
We are pleased to report record revenue of 4.2 billion dollars." -o /tmp/speaker1.aiff

say -v Daniel "Thank you. Total revenue came in at 4.2 billion. Operating expenses \
were 2.1 billion, resulting in a healthy margin." -o /tmp/speaker2.aiff

say -v Samantha "Thank you for that overview. Let me now open the floor for \
questions." -o /tmp/speaker3.aiff

ffmpeg -y \
  -i /tmp/speaker1.aiff -i /tmp/speaker2.aiff -i /tmp/speaker3.aiff \
  -filter_complex "[0:a][1:a][2:a]concat=n=3:v=0:a=1[out]" \
  -map "[out]" -ar 16000 -ac 1 /tmp/test.wav
\`\`\`

\`\`\`bash [Linux / Any Platform]
# Use any MP3 or WAV file you have:
cp /path/to/your/earnings-call.mp3 /tmp/test.mp3

# Or generate with espeak + ffmpeg:
espeak -w /tmp/speaker1.wav "Good morning everyone and welcome to the Q3 earnings call."
espeak -w /tmp/speaker2.wav "Thank you. Total revenue came in at 4.2 billion."
ffmpeg -y \
  -i /tmp/speaker1.wav -i /tmp/speaker2.wav \
  -filter_complex "[0:a][1:a]concat=n=2:v=0:a=1[out]" \
  -map "[out]" -ar 16000 -ac 1 /tmp/test.wav
\`\`\`

:::
```

---

### `/guide/first-run/process`

The `cargo run -- earnings process` section, identical to what you have now. Standalone page.

---

## Updated `rspress.config.ts` Sidebar

```typescript
"/guide/": [
  {
    text: "Getting Started",
    items: [
      { text: "Introduction", link: "/guide/introduction" },
      { text: "Quick Start", link: "/guide/quick-start" },
    ],
  },
  {
    text: "Installation",
    items: [
      { text: "Prerequisites", link: "/guide/installation/prerequisites" },
      { text: "macOS", link: "/guide/installation/macos" },
      { text: "Linux (Ubuntu/Debian)", link: "/guide/installation/linux" },
      { text: "Cloud (EC2 / Terraform)", link: "/guide/installation/cloud" },
    ],
  },
  {
    text: "MongoDB Setup",
    items: [
      { text: "Overview", link: "/guide/mongodb/overview" },
      { text: "Local (Atlas CLI)", link: "/guide/mongodb/local-atlas-cli" },
      { text: "Atlas Cloud", link: "/guide/mongodb/atlas-cloud" },
      { text: "Self-Hosted / Existing", link: "/guide/mongodb/self-hosted" },
    ],
  },
  {
    text: "Services",
    items: [
      { text: "STT Service", link: "/guide/services/stt" },
      { text: "Hugging Face Authentication", link: "/guide/services/hugging-face-auth" },
    ],
  },
  {
    text: "First Run",
    items: [
      { text: "Generate Test Audio", link: "/guide/first-run/test-audio" },
      { text: "Process an Earnings Call", link: "/guide/first-run/process" },
    ],
  },
],
```

---

## Why This Structure Works

| Problem with the current guide | How this fixes it |
|---|---|
| Linux users scroll past `brew install` and `say` commands | Platform-specific pages; tabbed code blocks where inline |
| Someone on Atlas Cloud reads through the Atlas CLI Docker setup | Three separate MongoDB pages — pick yours |
| EC2 / cloud deployment isn't addressed at all | Dedicated cloud page with GPU notes and Terraform reference |
| One long page is hard to link to from issues / READMEs | Each topic is a stable URL |
| Quick Start is overwhelming | Quick Start becomes a short routing page with 6 numbered steps linking out |

The quick start stays fast for someone who just wants to follow the happy path, and the deeper pages are there when the environment diverges.