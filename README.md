# Lontar

> A local-first knowledge base — Obsidian-compatible, built with Tauri 2 + Svelte 5 + Rust.

![CI](https://github.com/ajianaz/lontar/actions/workflows/ci.yml/badge.svg?branch=develop)
![Rust](https://img.shields.io/badge/Rust-edition_2024-orange)
![Svelte](https://img.shields.io/badge/Svelte-5-ff3e00)
![Tauri](https://img.shields.io/badge/Tauri-2-24c8d8)
![License](https://img.shields.io/badge/License-MIT-blue)

## Overview

Lontar is a desktop note-taking app that reads/writes plain Markdown files in an Obsidian-compatible vault structure — wikilinks (`[[links]]`), YAML frontmatter, tags, and folder hierarchy. Everything stays on your machine. No cloud, no accounts, no tracking.

**Target:** ARM64 (Raspberry Pi 5) primary, cross-platform (Linux, macOS, Windows).

## Tech Stack

| Layer | Technology |
|-------|-----------|
| **Shell** | [Tauri 2](https://v2.tauri.app/) — native webview, no Electron |
| **Frontend** | [Svelte 5](https://svelte.dev/) (runes) + TypeScript |
| **Editor** | [CodeMirror 6](https://codemirror.net/) with wikilink extension |
| **Backend** | Rust (edition 2024) |
| **Search** | [Tantivy](https://tantivy-search.github.io/) — full-text search engine |
| **Indexing** | Rayon parallel indexer — frontmatter, links, tags, graph |
| **FS Watch** | [notify](https://github.com/notify-rs/notify) with debouncing |
| **Theme** | Catppuccin Mocha (dark-first) |

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  Svelte 5 Frontend (webview)                        │
│  ┌──────────┐ ┌──────────┐ ┌────────────────────┐  │
│  │ Sidebar   │ │ TabBar   │ │ EditorPane (CM6)   │  │
│  │ FileTree  │ │          │ │ SidePanel          │  │
│  │ SearchBar │ │          │ │  ├ Backlinks       │  │
│  └──────────┘ └──────────┘ │  ├ Outline         │  │
│                            │  └ Tags             │  │
│                            └────────────────────┘  │
│  StatusBar                                          │
├────────────────── Tauri IPC ────────────────────────┤
│  Rust Backend                                       │
│  ┌──────────┐ ┌──────────┐ ┌────────┐ ┌────────┐  │
│  │ Vault    │ │ Indexer  │ │ Search │ │Watcher │  │
│  │ Manager  │ │ (rayon)  │ │(tantivy)│ │(notify)│  │
│  └──────────┘ └──────────┘ └────────┘ └────────┘  │
│  21 IPC commands via tauri::command                 │
└─────────────────────────────────────────────────────┘
```

## Features

### ✅ Implemented (P0: Foundation)

- **Vault management** — open, close, lock file, path traversal protection
- **File tree** — recursive directory traversal, CRUD for notes & folders
- **Atomic file writes** — safe writes via temp file + rename
- **FS watcher** — real-time file change detection with debouncing
- **Tantivy search** — full-text search with autocomplete suggestions
- **Rayon indexer** — parallel index of frontmatter, wikilinks, tags, graph
- **IPC layer** — 21 Tauri commands exposing all backend functionality
- **CI pipeline** — cargo fmt + clippy + test (Rust), typecheck + build (Svelte)

### 🔨 In Progress (P1: Editor Core)

- Svelte 5 app shell + layout
- CodeMirror 6 editor integration

### 📋 Planned

- **P1:** Wikilink/tag CM6 extensions, file tree sidebar, tab system, auto-save, backlinks, search UI, tag index
- **P2:** Graph view (D3.js), markdown preview, daily notes, command palette, outline panel, templates
- **P3:** Theme system, settings panel, CSP hardening, trash system, cross-platform installers
- **P4:** Canvas editor (Obsidian-compatible JSON)

## Project Structure

```
lontar/
├── src/                          # Svelte 5 frontend
│   ├── App.svelte                # Root layout
│   ├── app.css                   # Catppuccin Mocha theme
│   ├── lib/
│   │   ├── components/           # UI components (9 files)
│   │   ├── stores/               # Svelte 5 reactive stores
│   │   ├── cm6/                  # CodeMirror 6 setup + extensions
│   │   └── ts/                   # Types, IPC bridge, constants
│   └── main.ts                   # Entry point
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── main.rs               # Entry point
│   │   ├── lib.rs                # Tauri builder, plugin setup
│   │   ├── commands.rs           # 21 IPC commands (901 LOC)
│   │   ├── atomic_write.rs       # Safe file writes
│   │   ├── vault/                # Vault manager + lock file
│   │   ├── indexer/              # Rayon parallel indexer
│   │   ├── search.rs             # Tantivy search engine
│   │   └── watcher.rs            # FS watcher with debouncing
│   ├── Cargo.toml
│   └── tauri.conf.json
├── .github/workflows/ci.yml      # CI pipeline
├── package.json
└── README.md
```

## Development

### Prerequisites

- [Rust](https://rustup.rs/) (edition 2024 / stable)
- [Node.js](https://nodejs.org/) 20+
- Tauri system dependencies:
  - **Ubuntu/Debian:** `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libglib2.0-dev`
  - **macOS:** Xcode Command Line Tools (`xcode-select --install`)
  - **Windows:** Microsoft Visual Studio C++ Build Tools

### Setup

```bash
git clone https://github.com/ajianaz/lontar.git
cd lontar
npm install
```

### Dev

```bash
npm run tauri dev
```

### Build

```bash
npm run tauri build
```

### Quality Gates

```bash
# Rust
cd src-tauri
cargo fmt --all --check     # Formatting
cargo clippy --all-targets -- -D warnings  # Lint
cargo test --all            # Tests

# Frontend
npx tsc --noEmit            # Type check
npm run build               # Build
```

### Branch Strategy

```
main ──────── stable releases
  └── develop ─── active development (default branch)
       ├── feat/xxx
       ├── fix/xxx
       └── docs/xxx
```

All feature work happens on branches from `develop`, merged back via PR. CI runs on every push to `develop` and on all PRs targeting `develop`.

## Stats

| Component | Lines of Code |
|-----------|--------------|
| Rust backend | ~4,565 |
| Svelte frontend | ~933 |
| TypeScript/TS stores | ~639 |
| **Total** | **~6,137** |

## License

MIT
