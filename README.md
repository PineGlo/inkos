# InkOS — Phase 0 Scaffold (Tauri v2)

This scaffold targets **Tauri v2** + React (Vite) + Rust core.

## Quick Start (Windows PowerShell)
```powershell
cd ui
npm i
cd ..\src-tauri
cargo tauri dev
```

## Notes
- Config schema: `https://schema.tauri.app/config/2`
- `@tauri-apps/api` v2: `import { invoke } from '@tauri-apps/api/core'`
- DB is stored under OS-specific app data dir via `directories` crate.
