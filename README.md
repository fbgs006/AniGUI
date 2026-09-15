# AniGUI 🌸

AniGUI is a premium, beautifully-designed desktop anime companion app that merges the lightning-fast CLI streaming of `ani-cli` with a gorgeous, modern graphical interface.

Built with Tauri 2 and Rust, AniGUI lets you browse trending anime, watch episodes directly on your desktop, and automatically sync your progress with your AniList account.

## ✨ Features

- **Zero-Setup Install:** No Scoop, no terminal, no manual `ani-cli`/`mpv` install — AniGUI downloads its own private runtime the first time you launch it.
- **Real-Time Catalog:** Browse trending, highly-rated, and upcoming anime directly from AniList, or search the entire database instantly.
- **Lightning Fast Playback:** Seamlessly streams episodes via `ani-cli` into a native `mpv` video player window.
- **Download Manager:** Download episodes to your hard drive and manage or play them natively from the built-in Downloads tab.
- **Exact-Second Resume:** Native Lua scripts track exactly where you close the player — reopen an episode and it resumes from the exact second you left off.
- **Smart Auto-Sync (AniSkip integration):** Connect your AniList account and AniGUI will automatically update your progress when you finish an episode. Uses the AniSkip API to detect the ending song timestamp so your progress only syncs when you've genuinely finished.
- **Advanced Browse Filtering:** Filter by 18 genres with multi-select support, plus year, season, format, and sort order. A live badge shows how many genres are active.
- **5 Color Themes:** Switch between Purple, Crimson, Ocean, Emerald, and Monochrome themes from the Settings panel.
- **Related Media:** Jump to prequels, sequels, spin-offs, and movies directly from any anime's detail page.

## 🚀 Installation

No prerequisites — just download and run:

1. Go to the **[Releases](https://github.com/fbgs006/ani-gui/releases)** tab.
2. Download `AniGUI-setup.exe` (installer) or `AniGUI.exe` (portable — no installation needed).
3. Run it. On first launch, AniGUI downloads its own private copy of `ani-cli`, `mpv`, `fzf`, and a portable Git Bash into `%APPDATA%\AniGUI\runtime\` (~150-200MB, one time only) — no Scoop, no manual installs, nothing added to your system PATH.

Already have `ani-cli`/`mpv`/Git Bash installed and want AniGUI to use those instead? Click **"I already have these installed — skip"** on the setup screen, or set them manually as described below.

### Advanced: manual / Scoop-based setup

If you'd rather manage `ani-cli` and `mpv` yourself instead of using AniGUI's bundled runtime:

```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
Invoke-RestMethod -Uri https://get.scoop.sh | Invoke-Expression
scoop install git mpv ani-cli
```

Then in AniGUI's **⚙️ Settings → Bash / Git Bash Path**, point it at your own `bash.exe` (e.g. `C:\Program Files\Git\bin\bash.exe`) — an explicit path here always overrides the bundled runtime. You can re-trigger the bundled setup at any time from **Settings → Dependency Setup → Repair / Reinstall**.

### Building from Source

1. Clone this repository.
2. Install [Node.js](https://nodejs.org/) and [Rust](https://rustup.rs/).
3. Open a terminal inside the `anigui-tauri/` folder.
4. Run `npm install` to install frontend dependencies.
5. Run `npm run tauri dev` to launch in development mode.

## ⚙️ Settings

Click the **⚙️ Settings** button in the **top-right header**:

1. **AniList Token** — Click **Open AniList Login →** to get your token. Paste it here to enable sync and the "Watching" / "Planning" tabs.
2. **Bash / Git Bash Path** — Leave blank to use AniGUI's bundled runtime (recommended), or point it at your own `bash.exe` to use a system-installed `ani-cli`/`mpv` instead.
3. **Dependency Setup** — Shows whether AniGUI is running on its bundled runtime or a system install, with a **Repair / Reinstall** button if something goes wrong.
4. **Download Directory** — Choose where downloaded episodes are saved (defaults to `Downloads/AniGUI`).
5. **Auto-Sync** — Toggle on to silently sync progress when you finish an episode. Leave off for a confirm prompt each time.
6. **Theme** — Pick your preferred color theme.

## 🛠️ Architecture

| Layer | Technology |
|---|---|
| Frontend | TypeScript (modular — views/, components/), Vanilla CSS |
| Backend | Rust (Tauri 2) |
| Runtime bootstrap | Downloads a private `ani-cli`/`mpv`/`fzf`/Git Bash into `%APPDATA%\AniGUI\runtime\` on first launch (`src-tauri/src/bootstrap.rs`) |
| Streaming | `ani-cli` via Git Bash shell commands |
| Player | `mpv` with embedded Lua scripts for resume tracking |
| Anime Data | AniList GraphQL API |
| Skip Detection | AniSkip API |

**Frontend module structure:**
```
src/
├── main.ts              ← Boot & Tauri event wiring
├── types.ts / state.ts / utils.ts
├── components/          ← toast, sync-bar, settings
└── views/               ← browse, detail, sidebar, downloads
    (each paired with its own .css file)
```

---

*Note: This project is a collaboration between Human and AI. The Human (creator) acts as the Director and Project Manager — generating ideas, making design decisions, and finding issues — while the AI acts as the Lead Developer, writing code, building features, and fixing bugs.*

*Disclaimer: AniGUI is a graphical wrapper for ani-cli. It does not host or store any copyrighted video material.*
