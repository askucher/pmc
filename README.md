# Process Management Controller (PMC)

## Overview

PMC is a simple and easy to use PM2 alternative written in Rust. It provides a CLI, HTTP API, and WebUI to start, stop, restart, and manage processes with support for remote server management.

## Features

- Start, stop, restart, and remove processes
- List and monitor running processes with CPU/memory usage
- Auto-restart on crash with configurable limits
- File watching for auto-reload on changes
- Process log management with real-time streaming
- Save and restore process lists across daemon restarts
- Import/export process configurations (HCL format)
- Remote server management
- HTTP API with WebSocket support
- Optional WebUI and Prometheus metrics
- Token-based API authentication

## Usage

```bash
# Start a new process or restart an existing one
pmc start <id/name> or <script> [--name <name>] [--watch <path>]

# Stop a process (alias: kill)
pmc stop <id/name>

# Remove a process (aliases: rm, delete)
pmc remove <id/name>
pmc delete all

# List all processes (aliases: ls, status)
pmc list [--format <default|json|raw>]
pmc status

# Get detailed process info (alias: info)
pmc details <id/name> [--format <default|json|raw>]

# Get process environment variables (alias: cmdline)
pmc env <id/name>

# View process logs
pmc logs <id/name> [--lines <num>]

# Flush process logs (aliases: clean, log_rotate)
pmc flush <id/name>
pmc flush all

# Save all processes to dump file (alias: store)
pmc save

# Restore all processes from dump file (alias: resurrect)
pmc restore

# Import processes from HCL config (alias: add)
pmc import <path>

# Export process config to HCL (alias: get)
pmc export <id/name> [<path>]
```

### Daemon Management

```bash
# Start/restart daemon (aliases: daemon agent, daemon bgd)
pmc daemon start [--api] [--webui]

# Stop daemon
pmc daemon stop

# Check daemon health (aliases: info, status)
pmc daemon health [--format <default|json|raw>]

# Reset process index
pmc daemon reset
```

### Server Management

```bash
# Add a new remote server
pmc server new

# List configured servers
pmc server list [--format <format>]

# Remove a server
pmc server remove <name>

# Set default server
pmc server default [<name>]
```

Most process commands accept `--server <name>` to target a remote PMC instance, and `all` as an argument to apply to all processes.

For more command information, run `pmc --help`.

### Configuration

PMC stores its configuration in `~/.pmc/`:

- `config.toml` - Main configuration (shell, log paths, daemon settings)
- `servers.toml` - Remote server configurations
- `process.dump` - Saved process state
- `logs/` - Process log files (`<name>-out.log`, `<name>-error.log`)

### Installation

Pre-built binaries for Linux, MacOS, and WSL can be found on the [releases](releases) page.

There is no windows support yet. Install from crates.io using `cargo install pmc` (requires clang++)

#### Building

- Clone the project
- Open a terminal in the project folder
- Check if you have cargo (Rust's package manager) installed, just type in `cargo`
- If cargo is installed, run `cargo build --release`
- Put the executable into one of your PATH entries, usually `/bin/` or `/usr/bin/`
