# InkOS — Phase 0 Scaffold (Tauri v2)

InkOS is a Tauri v2 application scaffold that combines a **React (Vite)** front end with a **Rust** core. This repository provides the starting point for iterating on the InkOS product vision.

## Prerequisites

Ensure the following tools are installed:

- [Rust](https://www.rust-lang.org/tools/install)
- [Node.js](https://nodejs.org/) and npm (or [pnpm](https://pnpm.io) / [yarn](https://yarnpkg.com))
- Tauri v2 prerequisites for your platform (see the [official guide](https://tauri.app/v1/guides/getting-started/prerequisites))

## Project Structure

```
.
├── core            # Shared logic and utilities for the Rust core
├── docs            # Long-form documentation, architecture notes, and design specs
├── migrations      # Database migration files
├── src-tauri       # Tauri configuration and Rust backend entry point
└── ui              # React (Vite) frontend
```

## Setup

Install dependencies for both the frontend and backend:

```bash
cd ui
npm install
cd ../src-tauri
cargo build
```

## Running the App

During development you can run the app with the following command (from the repository root):

```bash
cd ui
npm run dev
```

In a second terminal, launch Tauri:

```bash
cd src-tauri
cargo tauri dev
```

## Testing

- UI: `npm test -- --watch=false`
- Rust core: `cargo test`

## Configuration Notes

- Config schema: `https://schema.tauri.app/config/2`
- `@tauri-apps/api` v2 usage: `import { invoke } from '@tauri-apps/api/core'`
- The database is stored under the OS-specific app data directory via the `directories` crate.

## Roadmap

See [ROADMAP.md](./ROADMAP.md) for the current development milestones and focus areas.

## Contributing

1. Fork the repository and create a feature branch.
2. Make your changes and ensure tests pass.
3. Submit a pull request describing your changes.
