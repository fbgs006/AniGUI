# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository layout

`anigui-tauri/` is the app — a Tauri 2 (Rust backend + TypeScript/Vite frontend) desktop GUI. All development happens there; the repo root only holds `README.md`, `LICENSE`, and CI/release workflows.

All commands below assume `cd anigui-tauri` first.

## Commands

```bash
npm install              # install frontend deps
npm run dev               # Vite dev server only (no Tauri window)
npm run tauri dev         # full app in dev mode (Rust + frontend, hot reload)
npm run build              # tsc typecheck + vite build (frontend only)
npm run tauri build        # produce the installer/portable .exe

cargo check --manifest-path src-tauri/Cargo.toml   # fast Rust typecheck (what CI runs)
cargo test --manifest-path src-tauri/Cargo.toml    # run Rust unit tests (in src-tauri/src/lib.rs, mod backend_helper_tests)
```

There is no frontend test runner configured — TypeScript correctness is checked only via `tsc` during `npm run build`.

CI (`.github/workflows/ci.yml`) runs on `windows-latest` and does exactly: `npm ci`, `npm run build`, `cargo check`. Releases (`.github/workflows/release.yml`) build on tag push via `tauri-apps/tauri-action`.

## Runtime dependencies (not npm/cargo packages)

The app is a GUI shell around two external CLI tools that must be present on the user's system and are invoked as subprocesses:
- **`ani-cli`** — does the actual anime scraping/streaming; invoked via `bash -lc "ani-cli ..."`.
- **`mpv`** — the video player `ani-cli` launches; AniGUI installs a Lua tracking script into mpv's `scripts/` directory (see below).
- **Git Bash (`bash.exe`)** — required because `ani-cli` is a bash script. Its path is auto-detected (`find_bash` in `lib.rs`) or set manually in Settings.

Keep this in mind when changing anything under `play_episode`/`start_download`: these functions shell out to bash and parse ANSI-laden stdout, they don't call a library.

## Architecture

### Backend (`src-tauri/src/lib.rs`, single file)

- All Tauri commands live in this one file and are registered in the `invoke_handler![...]` list in `run()` at the bottom — new commands must be added there.
- `AppState` holds the shared `Config` (Mutex-guarded, persisted to `%APPDATA%/anicli-gui/config.json`), a `player_active` flag preventing two simultaneous `mpv` launches, and a shared `reqwest::Client` (15s timeout) reused for all HTTP calls.
- All AniList access goes through `anilist_query()`, which POSTs GraphQL to `https://graphql.anilist.co`, retries on 429/rate-limit/transient errors (up to 2 retries with backoff), and treats a blank token as anonymous (`usable_anilist_token`) so Browse/Search still work logged-out.
- GraphQL query strings are built with the shared `MEDIA_FIELDS` fragment (see functions like `trending_query()`, `search_query()`, `advanced_search_query()`) — when adding a new AniList query, reuse this fragment rather than duplicating fields, and make sure `mediaListEntry { id progress status }` is included so logged-in progress renders on Browse/Search too (there's a regression test for this: `discovery_queries_request_viewer_progress_when_logged_in`).
- `play_episode`/`start_download` spawn `bash -lc "ani-cli ..."` in a background `std::thread`, minimize the main window during playback, and on exit read `%APPDATA%/AniGUI/last_watched.json` (written by the injected Lua script) to emit a `playback_finished` event with `{ epNum, percent, timePos, elapsed }`.
- `install_mpv_script()` runs once at startup and writes `anigui-tracker.lua` into mpv's script directory (auto-detecting Scoop's `portable_config/scripts` vs `%APPDATA%/mpv/scripts`). This script does the actual timestamp read/write to JSON files under `%APPDATA%/AniGUI/` — the Rust side only reads the final result after the player closes.
- Unit tests (`#[cfg(test)] mod backend_helper_tests`) cover version-comparison logic (`anicli_update_available`) and the anonymous-token/query-shape invariants above — extend these when touching that logic.

### Frontend (`src/`)

- `main.ts` is intentionally slim: it wires DOM events and Tauri event listeners (`player_closed`, `playback_finished`, `download_chunk`, `download_finished`) and delegates everything else. Follow this pattern — don't grow `main.ts`.
- `state.ts` exports a single mutable `state` object that every view/component reads and writes directly (no store/reducer layer). This is the one source of truth for cross-view data (current tab, sidebar list, selected media, playback/download progress, pending sync).
- `types.ts` mirrors the Rust `Config`/GraphQL shapes on the frontend — keep both in sync when changing `Config` fields in `lib.rs`.
- `views/` (`browse`, `detail`, `sidebar`, `downloads`, `calendar`, `profile`) each pair a `.ts` file with its own `.css` file and export loader functions (`loadBrowse`, `loadTab`, etc.) called from `main.ts`. `views/browse.ts` keeps a short-lived (2 min) in-memory cache to avoid re-hitting AniList on every tab click — follow that pattern for other expensive aggregate views rather than adding a global cache layer.
- `components/` holds small reusable UI: `toast`, `sync-bar` (the "sync this episode?" confirm banner), `settings` panel, `shortcuts` overlay, and the `updater`/`anicli-updater` self-update checks.
- The AniSkip-based "did the user actually finish this episode" logic in `main.ts`'s `playback_finished` listener is duplicated in the dev-only `__testSync()` helper at the bottom of the same file — if you change the finish-detection logic (percent threshold, AniSkip ED lookup, elapsed-time guard), update both, since the point of `__testSync()` is to exactly simulate this code path for manual testing without needing to run a full playback.

### Config flow

Config is Rust-owned: `get_config`/`save_config` commands round-trip a JSON blob between `%APPDATA%/anicli-gui/config.json` and the frontend `Config` type. The frontend never writes files directly — every settings change goes through `invoke("save_config", {...})`.
