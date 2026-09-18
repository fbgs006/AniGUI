# AniGUI 🌸

AniGUI is a premium, beautifully-designed desktop anime companion app that merges the lightning-fast CLI streaming of `ani-cli` with a gorgeous, modern graphical interface.

Built with Tauri 2 and Rust, AniGUI lets you browse trending anime, watch episodes directly on your desktop, and automatically sync your progress with your AniList account.

## ✨ Features

- **Cinematic UI:** A full art-first redesign — an icon rail replaces the old tab bar, a Home screen greets you with a full-bleed banner of whatever you're mid-episode on, and a dark, coral-accented theme runs through every screen.
- **Zero-Setup Install (Windows):** No Scoop, no terminal, no manual `ani-cli`/`mpv` install — AniGUI downloads its own private runtime the first time you launch it, and runs everything without popping a visible console window.
- **Real-Time Catalog:** Browse trending, highly-rated, and upcoming anime directly from AniList, or search the entire database instantly.
- **Lightning Fast Playback:** Seamlessly streams episodes via `ani-cli` into a native `mpv` video player window.
- **Download Queue:** Queue up multiple episodes at once — they download one at a time in the background with live per-item progress, and a failed download auto-retries before you ever have to touch it.
- **Bulk List Management:** Multi-select shows on your AniList lists and move a batch from Watching to Dropped, mark a whole season watched in one click, or reshuffle your Planning list — no more clicking into each show individually.
- **Local Watch-History Stats:** A stats view built from your own play history — episodes watched, approximate hours, most-rewatched show, and a top-genres breakdown — plus one-click CSV export.
- **Exact-Second Resume:** Native Lua scripts track exactly where you close the player — reopen an episode and it resumes from the exact second you left off.
- **Smart Auto-Sync (AniSkip integration):** Connect your AniList account and AniGUI will automatically update your progress when you finish an episode. Uses the AniSkip API to detect the ending song timestamp so your progress only syncs when you've genuinely finished.
- **Advanced Browse Filtering:** Filter by 18 genres with multi-select support, plus year, season, format, and sort order. A live badge shows how many genres are active.
- **6 Color Themes:** Coral (default), Purple, Crimson, Ocean, Emerald, and Monochrome — switch anytime from the Settings panel.
- **Related Media:** Jump to prequels, sequels, spin-offs, and movies directly from any anime's detail page.

## 📸 Screenshots

| Home | Browse |
|---|---|
| ![Home screen](screenshots/home.png) | ![Browse screen](screenshots/browse.png) |

| Airing This Week | Settings |
|---|---|
| ![Airing this week](screenshots/week.png) | ![Settings panel](screenshots/settings.png) |

## 🚀 Installation

Every tagged release publishes builds for **both Windows and Linux** — check the **[Releases](https://github.com/fbgs006/ani-gui/releases)** tab. Pick your platform below.

### Windows

No prerequisites — just download and run.

1. Download `AniGUI-setup.exe` (installer) or `AniGUI-Portable.exe` (portable, no installation needed).
2. Run it. On first launch, AniGUI downloads its own private copy of `ani-cli`, `mpv`, `fzf`, and a portable Git Bash into `%APPDATA%\AniGUI\runtime\` (~150-200MB, one time only). Nothing is added to your system PATH.

Already have `ani-cli`/`mpv`/Git Bash installed and want AniGUI to use those instead? Click **"I already have these installed — skip"** on the setup screen, or set the path manually later (see [Settings](#️-settings)).

### Linux

The zero-setup bundled runtime is **Windows-only** — on Linux you install `mpv` and `ani-cli` yourself first, then run AniGUI. `bash` is already your system shell, so there's nothing to configure there.

**1. Install the two runtime dependencies:**

Arch (and Arch-based distros):
```bash
sudo pacman -S mpv
yay -S ani-cli          # or paru, or any AUR helper
```

Ubuntu / Debian:
```bash
sudo apt install mpv
git clone "https://github.com/pystardust/ani-cli.git"
sudo install -Dm755 ani-cli/ani-cli /usr/local/bin/ani-cli
```

**2. Get AniGUI itself** — pick whichever you prefer:

| Method | What to do |
|---|---|
| **AppImage** (any distro, incl. Arch) | Download the `.AppImage` from Releases, then `chmod +x AniGUI-*.AppImage && ./AniGUI-*.AppImage` |
| **.deb** (Ubuntu/Debian) | Download the `.deb` from Releases and install with `sudo apt install ./AniGUI-*.deb` |
| **.rpm** (Fedora) | Download the `.rpm` from Releases and install with `sudo dnf install ./AniGUI-*.rpm` |
| **Build from source** | See [Building from Source](#-building-from-source) below — useful on Arch since there's no native `.pacman` package, or if you want the latest unreleased code |

### Advanced: manual / Scoop-based setup (Windows)

If you'd rather manage `ani-cli` and `mpv` yourself instead of using AniGUI's bundled runtime:

```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
Invoke-RestMethod -Uri https://get.scoop.sh | Invoke-Expression
scoop install git mpv ani-cli
```

Then in AniGUI's **You → Settings** (⚙ in the icon rail), point **Bash / Git Bash Path** at your own `bash.exe` (e.g. `C:\Program Files\Git\bin\bash.exe`) — an explicit path here always overrides the bundled runtime. You can re-trigger the bundled setup at any time from **Settings → Dependency Setup → Repair / Reinstall**.

## 🛠️ Building from Source

Building from source works the same on every OS: clone, install deps, run the Tauri CLI. Linux just needs a few extra system packages first (Windows and macOS don't).

**1. Install the toolchain:**
- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://rustup.rs/)

**2. Linux only — install Tauri's system dependencies:**

Arch:
```bash
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl \
  appmenu-gtk-module libappindicator-gtk3 librsvg xdotool
```

Ubuntu / Debian:
```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
  patchelf build-essential curl wget file libxdo-dev libssl-dev
```

**3. Clone and run:**

```bash
git clone https://github.com/fbgs006/ani-gui.git
cd ani-gui/anigui-tauri
npm install
npm run tauri dev      # launches the full app in dev mode with hot reload
```

**4. To produce an installable build instead of dev mode:**

```bash
npm run tauri build
```

This puts a native installer/portable build under `src-tauri/target/release/bundle/` — an `.AppImage`/`.deb` on Linux, an `.exe`/`.msi` on Windows.

> Frontend-only iteration: `npm run dev` starts just the Vite dev server (no Tauri window, no Rust). `npm run build` runs `tsc` + a Vite build without producing a native app. Useful for quick UI work, but you need `npm run tauri dev` to actually exercise playback/downloads since those are Rust commands.

## 🧭 Using AniGUI

The left rail is your nav: **Home** (continue watching + plan-to-watch, or trending if you're not logged in), **Browse** (trending/seasonal/all-time, filterable by genre/year/format), **Watching** (jumps straight to whatever you're mid-episode on), **Week** (7-day airing calendar), **Offline** (downloaded files + the active download queue), and **You** (your AniList profile — Stats, History, and Manage).

Press **?** anytime for the full keyboard-shortcut list.

## ⚙️ Settings

Click **You → Settings** (or the ⚙ icon at the bottom of the rail):

1. **AniList Token** — Click **Open AniList Login →** to get your token. Paste it here to enable sync, Home's continue-watching row, and bulk list management.
2. **Bash / Git Bash Path** (Windows) — Leave blank to use AniGUI's bundled runtime (recommended), or point it at your own `bash.exe` to use a system-installed `ani-cli`/`mpv` instead.
3. **Dependency Setup** (Windows) — Shows whether AniGUI is running on its bundled runtime or a system install, with a **Repair / Reinstall** button if something goes wrong.
4. **Download Directory** — Choose where downloaded episodes are saved (defaults to `Downloads/AniGUI`).
5. **Auto-Sync** — Toggle on to silently sync progress when you finish an episode. Leave off for a confirm prompt each time.
6. **Theme** — Pick your preferred color theme.

## 🏗️ Architecture

| Layer | Technology |
|---|---|
| Frontend | TypeScript (modular — views/, components/), Vanilla CSS |
| Backend | Rust (Tauri 2) |
| Runtime bootstrap (Windows only) | Downloads a private `ani-cli`/`mpv`/`fzf`/Git Bash into `%APPDATA%\AniGUI\runtime\` on first launch (`src-tauri/src/bootstrap.rs`) |
| Streaming | `ani-cli` via a `bash` shell command (spawned window-free) |
| Player | `mpv` with embedded Lua scripts for resume tracking |
| Anime Data | AniList GraphQL API |
| Skip Detection | AniSkip API |

**Frontend module structure:**
```
src/
├── main.ts              ← Boot & Tauri event wiring
├── types.ts / state.ts / utils.ts
├── components/          ← toast, sync-bar, settings, shortcuts, download-queue, runtime-setup, updater
└── views/                ← home, browse, detail, downloads, calendar, profile
    (each paired with its own .css file)
```

**CI/CD:**
- Every push runs `npm run build` + `cargo check` on `windows-latest` as a fast correctness gate.
- Every tag push (`v*`) builds full release bundles on **both** `windows-latest` and `ubuntu-22.04`, and publishes them as a draft GitHub Release.

---

*Note: This project is a collaboration between Human and AI. The Human (creator) acts as the Director and Project Manager — generating ideas, making design decisions, and finding issues — while the AI acts as the Lead Developer, writing code, building features, and fixing bugs.*

*Disclaimer: AniGUI is a graphical wrapper for ani-cli. It does not host or store any copyrighted video material.*
