# INKOS — Full Master Development Document
*A Local-First, AI-Augmented Productivity Operating System within Your OS*

---

## 1. Vision & Philosophy

### 1.1. The Core Idea

InkOS aims to be the **personal cognitive operating system** — a unified, private, and intelligent workspace that manages your notes, tasks, calendar, files, and communications, while continuously learning from your workflows.  

Unlike conventional productivity suites, InkOS is **local-first** and **agentic**: it observes your activities, builds a semantic understanding of your work, and proposes optimizations and summaries — all without leaving your device or sending data to the cloud (unless you explicitly allow it).

In essence, InkOS transforms your desktop into a **self-contained productivity ecosystem**, where AI acts as a cooperative assistant rather than an opaque black box.

---

### 1.2. Design Principles

| Principle | Explanation |
|------------|--------------|
| **Local-First** | Every operation — AI, search, storage — happens on your machine. Cloud features are *optional exports*, not dependencies. |
| **Transparent AI** | All AI actions are visible, logged, and reversible. Users see what the model sees and does. |
| **Composable Architecture** | Everything is modular. Notes, calendar, email, and AI all share the same APIs and data model. |
| **Explainability by Design** | Every function and error includes a human-readable explanation — useful for users and developers alike. |
| **Keyboard-First Workflow** | The global Command Palette (Ctrl/Cmd + Space) provides fast, text-driven access to everything. |
| **Agentic but Controlled** | Agents can propose, summarize, and optimize, but cannot make unapproved code or data changes. |
| **Sustainable Complexity** | Core functions come first. Advanced AI, automation, and visualization layers build on solid foundations. |

---

### 1.3. Long-Term Vision

The ultimate goal is to give every user a **persistent digital memory** and **intelligent workspace** that grows with them:

- A unified database of everything you’ve created, read, or scheduled.
- A second brain capable of recalling ideas, commitments, and documents contextually.
- A nightly process that writes your “auto-journal” — summarizing your progress and patterns.
- A safe environment where agents improve organization, propose automations, and even self-debug.

InkOS blurs the boundary between an app suite and an operating system: it’s a *personal OS for cognition and productivity*.

---

## 2. System Overview

InkOS operates as a **desktop application** built with **Tauri (Rust backend)** and **React (frontend)**.  
The application behaves like a micro-OS running inside your host OS:

| OS Layer Analogy | InkOS Equivalent |
|------------------|------------------|
| Kernel | Core service (Rust) — handles DB, workers, and AI runtime |
| File System | Unified workspace database (SQLite + Vector store) |
| Shell | Command Palette and React UI |
| System Calls | Typed IPC API (`v1/*`) |
| Daemons | Workers (OCR, embeddings, sync, auto-doc) |
| Package Manager | Plugin & agent subsystem |
| User Space Apps | Notes, Calendar, Mail, Files, Browser, AI Chat |

---

## 3. Architecture (Detailed)

### 3.1. Layer Diagram

UI (React/Tauri)
├── Command Palette Overlay
├── Notes / Calendar / Tasks / Search modules
├── Supervisor Panel (agent approvals)
├── Debug Console (AI explanations)
└── Tray + Notifications

Core Service (Rust)
├── SQLite + Vector Store (Unified DB)
├── Job Queue + Scheduler
├── API Endpoints (v1/*)
├── Permission Manifest + Policy Enforcer
├── Plugin Manager
└── Agent Runtime (restricted)

Workers
├── OCR (Tesseract)
├── Embeddings (ONNX / local LLM)
├── Auto-Documentation (nightly)
├── File Indexer
└── Email Sync

Agent Runtime
├── Task Manager
├── Rules & Constraints (YAML manifest)
├── Proposal Engine (diff generator)
├── Learning Memory
└── Interface with AI Orchestrator

---

### 3.2. Data Flow Summary

1. **User Input → UI Layer**  
   Commands entered through the palette, keyboard shortcuts, or module UIs.

2. **UI → Core Service (IPC)**  
   Tauri `invoke()` calls reach Rust handlers (namespace `v1/*`).

3. **Core → Database / Workers**  
   Core validates, writes to SQLite, and may enqueue background jobs (e.g., embeddings or OCR).

4. **Workers → Event Log**  
   Each worker runs independently, reporting structured logs and results back to DB.

5. **Agents → Proposal System**  
   Agents read logs and data, analyze patterns, and write `agent_proposals` entries.

6. **Supervisor UI → User Review**  
   User reviews proposals → approves/rejects.  
   Approved proposals are applied by the Core safely via declarative diffs.

7. **Nightly Auto-Doc Job**  
   Scheduler runs at 02:00; it collects the day’s activity, summarizes via AI, and stores a “logbook” note + timeline data.

---

### 3.3. Technology Stack

| Layer | Tooling |
|-------|----------|
| **Backend Core** | Rust + Tauri + rusqlite + tokio |
| **Frontend** | React (Vite) + TypeScript + Zustand |
| **Search** | SQLite FTS5 + ONNX embeddings (MiniLM/E5) |
| **AI Models** | Local via Ollama or llama.cpp; optional API fallback |
| **OCR** | tesseract.js (local WASM worker) |
| **Voice (future)** | whisper.cpp local STT |
| **Data Store** | Single SQLite file per workspace |
| **Plugins** | WASI-style sandbox or limited Node runtime |
| **Security** | OS keychain for secrets + permission manifest |

---

## 4. Core Subsystems

### 4.1. Unified Knowledge Graph

All data — notes, calendar events, tasks, files, and emails — is stored as nodes in SQLite tables, connected by the `links` table:

| Column | Description |
|---------|--------------|
| `src_type` / `dst_type` | Entity types (“note”, “task”, “file”, etc.) |
| `rel` | Relationship type (“mentions”, “derived-from”, “attached-to”) |
| `created_at` | Timestamp of relationship creation |

The graph enables:
- Semantic linking between any items
- Graph visualization
- AI recall and summarization across contexts
- Auto-linking by similarity (embedding distance)

---

### 4.2. Job Queue & Worker System

- Implemented via `jobs` table with durable states: `queued`, `running`, `done`, `error`.  
- Workers poll the DB for jobs assigned to their type (`ocr`, `embed`, `autosummary`, etc.).  
- Jobs can be scheduled or run immediately.  
- Each job writes to `event_log` on start, progress, and completion.  
- Fault-tolerant: crashed workers can resume unfinished jobs on restart.

---

### 4.3. AI Orchestrator

A router layer that decides *which* model or provider to use based on policy and availability.

- **Priority order:** Local model → Cached results → Cloud provider (if allowed).  
- **Standard interface:** `ai.summarize`, `ai.embed`, `ai.chat`, `ai.explain`.  
- **Provider plugins:** each model defines latency, cost, and capability metadata.  
- **Fallback chain:** if a local model fails, queue a retry with smaller batch or fallback provider.

---

### 4.4. Agentic Runtime

Responsible for self-improvement and maintenance.

**Capabilities:**
- Read any non-private data (notes, logs, jobs).
- Write only to `agent_proposals`.
- Request embeddings or AI summaries through orchestrator.
- Produce human-readable markdown proposals with metadata (title, rationale, estimated impact).

**Restrictions:**
- Cannot modify code or DB directly.
- No shell or network access unless explicitly granted in its manifest.
- Time-boxed and rate-limited to prevent runaway loops.

---

### 4.5. AI-Assisted Debugging

Each module uses structured error codes (`NTE-1002`, `DB-1001`, etc.) and logs them into `event_log` with an explanatory field.  
The Debug Console UI allows developers to:

- Filter logs by code, module, or time.
- View `explain` fields (“why/what-now”).
- Ask the local AI to summarize logs and suggest fixes.

This makes the entire system self-documenting as it runs.

---

### 4.6. Auto-Documentation & Memory Timeline

**Nightly 2 AM job flow:**

1. Collect all events, edits, and tasks completed that day.  
2. Summarize them using local AI models.  
3. Write a new `notes` entry titled `Daily Log YYYY-MM-DD`.  
4. Generate or update the timeline dataset (`timeline.json` or table).  
5. Link all referenced items via the `links` table.  
6. Add relevant embeddings for semantic recall (“What did I do last Tuesday?”).

Result: a self-maintaining logbook and visual timeline of your life and work.

---

## 5. Module-Level Design

### 5.1. Notes Module
- Markdown + Canvas hybrid editor.
- OCR button for handwritten content (Tesseract).
- Version history per note.
- Inline AI actions: summarize, outline, extract tasks.
- Linked directly with planner (convert highlight → task).

### 5.2. Calendar & Planner
- Local events + tasks stored in DB.
- Smart planner: uses constraints (work hours, energy levels, priorities).
- “Plan My Day” button fills free time blocks.
- Optional ICS export/import.
- Integration with auto-doc (adds daily summaries).

### 5.3. Search & Recall
- Global search bar (inside Command Palette).
- Hybrid results combining FTS5 and vector similarity.
- Result types: note, event, task, file, email, agent proposal.
- Contextual snippets and link previews.
- Supports semantic questions (“show related to Project Orion”).

### 5.4. Files Module
- Configurable watched folders.
- Extract text from PDFs, docs, images (OCR).
- Auto-tagging based on content similarity.
- Smart collections (recent edits, research materials).

### 5.5. Email Module (Phase 1)
- Local IMAP sync → SQLite cache.
- Offline search and triage.
- Drafting with AI suggestions.
- Smart labels (Action / Reference / Waiting).

### 5.6. Browser & Clipper (Phase 1)
- Embedded minimal browser.
- Reader mode → Save to notes.
- Quick capture to “Inbox”.
- AI summarization and citation extraction.

### 5.7. Second-Brain Chat (Phase 1)
- Conversational interface using local context.
- “Memory-aware” queries across all entities.
- Uses retrieval-augmented generation (RAG) pipeline.
- Can create or link notes from conversation.

---

## 6. Data Schema Overview (Simplified)

| Table | Description |
|--------|--------------|
| `notes`, `note_blocks` | Notes and content blocks |
| `events`, `tasks` | Calendar + to-dos |
| `files` | Indexed file metadata |
| `emails` | Mail cache |
| `links` | Graph edges between entities |
| `embeddings` | Vector representations |
| `fts` | FTS5 virtual table for keyword search |
| `jobs` | Worker queue |
| `event_log` | Logs, errors, debug traces |
| `agent_proposals` | Agent-generated improvement suggestions |

All IDs are UUID strings. Time fields are UNIX ms timestamps.  
Foreign keys enforce consistency.  
Triggers keep `fts` and `embeddings` synchronized.

---

## 7. User Interface Philosophy

1. **Command Palette** is the “Start Menu” and “Terminal” of InkOS — one place to launch anything.  
2. **Sidebar Modules** for quick navigation (Notes, Calendar, Files, Agents).  
3. **Minimal Visual Clutter** — no unnecessary chrome; rely on shortcuts.  
4. **Explainability Panels** — any AI result can be expanded to show its source or reasoning.  
5. **Dark/Light Modes** for comfort and accessibility.

---

## 8. Logging, Debugging & Explainability

- Every log entry includes machine data **and** a human-language explanation.  
- Error codes are stable and documented.  
- The Debug Console can:
  - View structured logs.
  - Ask local AI for a plain-English summary.
  - Auto-group repetitive errors for pattern analysis.
- The system encourages developers to *teach through comments*:  
  All core files contain rich inline comments describing logic and trade-offs.

---

## 9. Security Model

| Aspect | Policy |
|--------|---------|
| **Data Privacy** | Everything local by default. No background internet traffic. |
| **Encryption** | Secrets in OS keychain; user vaults optional encrypted. |
| **Agents** | Sandboxed with explicit manifests (no arbitrary code execution). |
| **Plugins** | Limited FS & network scopes; permissions prompt on install. |
| **Updates** | Signed updates; delta diff via Tauri auto-updater. |
| **Telemetry** | None by default. User may opt-in for anonymous metrics. |

---

## 10. Development Roadmap

| Phase | Focus | Deliverables |
|-------|--------|--------------|
| **Phase 0 — Core Kernel** | Local DB, job queue, search, AI router, logging, auto-doc | <ul><li>Running Tauri app with DB & IPC</li><li>Command Palette operational</li><li>Daily logbook & timeline working</li><li>AI Debugger integrated</li></ul> |
| **Phase 1 — Expansion** | User modules (Notes, Planner, Chat, Files, Email) | <ul><li>Semantic search across modules</li><li>Second-brain chat</li><li>Smart planner</li></ul> |
| **Phase 2 — Connectivity** | Voice, scripting, plugin system, sync | <ul><li>Plugin SDK released</li><li>Optional cloud connectors</li></ul> |
| **Phase 3 — Intelligence** | Visualization & analytics | <ul><li>Knowledge graph viewer</li><li>Goal planner & retrospectives</li><li>Productivity metrics</li></ul> |

**Maintenance milestones:**
- Biweekly integration builds.
- Monthly AI model evaluation.
- Quarterly roadmap review.

---

## 11. Coding & Documentation Standards

- **Languages:** Rust for backend, TypeScript/React for UI.  
- **Comments:** Section headers (`// 1)`, `// 2)`) + rationale for design decisions.  
- **Testing:**  
  - Unit tests for every core module.  
  - Integration tests for IPC flows.  
  - UI tests for critical commands.  
- **Docs:**  
  - `/docs/architecture.md` — diagrams  
  - `/docs/api.md` — IPC contract reference  
  - `/docs/error-codes.md` — all stable codes  
  - `/docs/logging.md` — example logs  
- **Versioning:** Semantic versioning for app; incremental DB migrations.

---

## 12. Developer Workflow

1. **Clone & bootstrap:** `npm install && cargo build`  
2. **Run:** `npm run tauri dev`  
3. **Database migrations:** auto-run on startup.  
4. **Debug:** open Debug Console → tail logs → ask AI for explanations.  
5. **Workers:** run automatically on threadpool; can be started manually for testing.  
6. **Agents:** run in dry mode by default (generate proposals only).  
7. **Docs:** read inline comments + `/docs` folder for context.  

---

## 13. Example Daily User Flow (Experience Vision)

1. Morning: User opens InkOS; command palette shows “Good morning, here’s your schedule.”  
2. During work: Takes handwritten notes → OCR converts to text → AI summarizes → tasks auto-linked to planner.  
3. Afternoon: Agent notices several tasks past due → proposes new plan for tomorrow.  
4. Evening: User closes laptop. At 2 AM, auto-doc job runs → summarizes everything done → adds to timeline and logbook.  
5. Next day: User asks, “What did I do yesterday?” → Second-brain chat summarizes with links and context.

---

## 14. Future Directions

| Area | Potential Evolution |
|-------|----------------------|
| **Unified Memory Timeline** | Visual 3D or radial timeline showing projects, notes, and milestones. |
| **Adaptive Agents** | Agents that tune their own parameters based on feedback. |
| **Cross-Device Sync** | Optional peer-to-peer synchronization for mobile/desktop parity. |
| **Personal RPA Layer** | GUI recorder + AI planner for repetitive desktop tasks. |
| **Knowledge Galaxy Visualization** | Interactive 3D map of your personal knowledge graph. |
| **Learning Integration** | Generate study flashcards or summaries from your own notes. |

---

## 15. Repository Layout

inkos/
├─ core/
│ ├─ src/
│ │ ├─ api/v1.rs
│ │ ├─ db/
│ │ ├─ workers/
│ │ ├─ agents/
│ │ ├─ logging.rs
│ │ └─ errors.rs
│ └─ Cargo.toml
├─ ui/
│ ├─ src/
│ │ ├─ components/
│ │ ├─ modules/
│ │ ├─ lib/api.ts
│ │ └─ state/
│ └─ package.json
├─ migrations/
│ └─ 0001_init.sql
├─ docs/
│ ├─ architecture.md
│ ├─ error-codes.md
│ ├─ logging.md
│ └─ api.md
└─ INKOS_MASTER_DEV_DOC_FULL.md

---

## 16. Guiding Ethos (Cultural Rules for Development)

1. **Everything must be understandable** — if code needs AI to explain it, add comments until it doesn’t.  
2. **Privacy is sacred** — no hidden telemetry, ever.  
