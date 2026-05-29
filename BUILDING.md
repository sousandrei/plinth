# Building Plinth

Instructions for running Plinth locally and building a distributable app.

---

## Prerequisites

Install the following before anything else:

- [Rust](https://rustup.rs) (stable toolchain)
- [Bun](https://bun.sh)
- [Task](https://taskfile.dev) — task runner used throughout the project

On macOS you'll also need Xcode Command Line Tools:

```bash
xcode-select --install
```

---

## Local Development

Clone the repo and install frontend dependencies:

```bash
git clone https://github.com/sousandrei/plinth
cd plinth
bun install
```

Start the app in development mode:

```bash
task dev
```

This boots the Tauri dev window with hot-reload for both the frontend and backend.

If you have a CUDA-capable GPU and want to use it for model training:

```bash
task dev:cuda
```

---

## Building a Release

To produce a native installable bundle for your platform:

```bash
task package
```

With CUDA support:

```bash
task package:cuda
```

The output will be placed in `src-tauri/target/release/bundle/` — you'll find a `.dmg` on macOS, `.exe` / `.msi` on Windows, and `.deb` / `.rpm` / `.AppImage` on Linux.

---

## Linting & Formatting

Before committing, make sure everything is clean:

```bash
task lint      # Biome + TypeScript + Cargo Clippy
task format    # Auto-fix frontend and backend
```

Both commands run checks across the full stack in one go.
