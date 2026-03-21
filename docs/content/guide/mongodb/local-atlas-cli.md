# Local MongoDB with Atlas CLI

The Atlas CLI spins up a full-featured local deployment inside Docker — no cloud account needed.

## Prerequisites

### Install the Atlas CLI

**macOS**

```bash
brew install mongodb-atlas-cli
```

**Linux**

```bash
# See https://www.mongodb.com/docs/atlas/cli/current/install-atlas-cli/
curl -fsSL https://atlascli.mongodb.com/install.sh | bash
```

### Install a Docker-compatible runtime

**macOS**

```bash
brew install colima docker
colima start
```

**Linux**

```bash
# Docker Engine must be installed and running
# See https://docs.docker.com/engine/install/
sudo systemctl start docker
```

## Create the deployment

```bash
atlas local setup vetta-local --port 27017 --bindIpAll
```

On first run the CLI pulls container images. Once ready:

```text
Deployment vetta-local created.
```

## Export environment variables

```bash
export MONGODB_URI="mongodb://localhost:27017/?directConnection=true"
export MONGODB_DATABASE="vetta"
```

:::warning
Both variables must be set in every shell session. Consider adding them to your
`~/.bashrc`, `~/.zshrc`, or a local `.env` file.
:::

## Manage the deployment

```bash
atlas local list                    # Check status
atlas local pause vetta-local       # Stop (data preserved)
atlas local start vetta-local       # Resume
atlas local delete vetta-local      # Remove (data lost)
```