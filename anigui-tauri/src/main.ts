// ─── AniGUI – Boot & Event Wiring ─────────────────────────────────────────────
// This file is intentionally slim. All view logic lives in src/views/,
// all components in src/components/, and shared state in src/state.ts.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Config, RuntimeInstallProgress, RuntimeStatus } from "./types";
import { state } from "./state";
import { el } from "./utils";

// Views
import { loadBrowse } from "./views/browse";
import { loadDownloads } from "./views/downloads";
import { loadTab, loadMore } from "./views/sidebar";
import { resetPlayButtons } from "./views/detail";
import { loadCalendar } from "./views/calendar";
import { loadProfile } from "./views/profile";

// Utilities & Components
import { wireShortcuts, injectShortcutsOverlay, toggleOverlay } from "./components/shortcuts";
import { checkForUpdate } from "./components/updater";
import { checkAniCliVersion } from "./components/anicli-updater";

// Components
import { toast } from "./components/toast";
import { wireSyncBar, showSyncBar } from "./components/sync-bar";
import { wireSettings, updateLoginStatus, fetchViewerName } from "./components/settings";
import { openRuntimeSetup, wireRuntimeSetup, handleRuntimeProgress } from "./components/runtime-setup";

async function init() {
  // ── Config ──────────────────────────────────────────────────────────────────
  state.config = await invoke<Config>("get_config");
  document.body.setAttribute("data-theme", state.config.theme);
  updateLoginStatus();
  fetchViewerName(); // non-blocking

  // ── Runtime setup (ani-cli / mpv / fzf / Git Bash bootstrap) ─────────────────
  wireRuntimeSetup();
  try {
    const runtimeStatus = await invoke<RuntimeStatus>("check_runtime_status");
    if (!runtimeStatus.all_present && !runtimeStatus.system_fallback_present && !runtimeStatus.skip_auto_setup) {
      openRuntimeSetup(runtimeStatus);
    }
  } catch { /* non-critical — Settings still offers a manual bash_path override */ }

  // ── Inject & wire global components ──────────────────────────────────────────
  injectShortcutsOverlay();
  wireShortcuts();
  checkForUpdate();
  checkAniCliVersion();

  // ── Initial tab ─────────────────────────────────────────────────────────────
  // Default to the user's watch list if they're already logged in.
  loadTab(state.config.anilist_token ? "continue" : "trending");

  // ── Tab buttons ─────────────────────────────────────────────────────────────
  document.getElementById("tab-trending")!.addEventListener("click", () => loadTab("trending"));
  document.getElementById("tab-continue")!.addEventListener("click", () => loadTab("continue"));
  document.getElementById("tab-planning")!.addEventListener("click", () => loadTab("planning"));

  // ── Browse, Downloads, Calendar ──────────────────────────────────────────────
  document.getElementById("btn-browse")!.addEventListener("click", loadBrowse);
  document.getElementById("btn-downloads")!.addEventListener("click", loadDownloads);
  document.getElementById("btn-calendar")!.addEventListener("click", loadCalendar);
  document.getElementById("btn-shortcuts")!.addEventListener("click", toggleOverlay);

  // ── Profile (login-status) ────────────────────────────────────────────────────
  // Logged in  → open Profile on Stats tab
  // Not logged in → open Profile on History tab (history works without login)
  document.getElementById("login-status")!.addEventListener("click", () => {
    loadProfile(state.config.anilist_token ? "stats" : "history");
  });

  // ── Sidebar infinite scroll ──────────────────────────────────────────────────
  document.getElementById("sidebar-list")!.addEventListener("scroll", (e) => {
    const el = e.target as HTMLElement;
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 80) loadMore();
  });

  // ── Search ───────────────────────────────────────────────────────────────────
  let searchTimer: ReturnType<typeof setTimeout>;
  document.getElementById("search-input")!.addEventListener("input", (e) => {
    const q = (e.target as HTMLInputElement).value.trim();
    clearTimeout(searchTimer);
    if (q.length < 2) { if (!q) loadTab("trending"); return; }
    searchTimer = setTimeout(() => loadTab("search", q), 400);
  });

  // ── Settings ─────────────────────────────────────────────────────────────────
  wireSettings();

  // ── Download modal close ─────────────────────────────────────────────────────
  document.getElementById("close-download")!.addEventListener("click", () =>
    document.getElementById("modal-download")!.classList.remove("open")
  );
  document.getElementById("close-download2")!.addEventListener("click", () =>
    document.getElementById("modal-download")!.classList.remove("open")
  );

  // ── Sync confirm bar ─────────────────────────────────────────────────────────
  wireSyncBar();

  // ── Tauri Events ─────────────────────────────────────────────────────────────

  await listen("runtime_install_progress", (event: any) => {
    handleRuntimeProgress(event.payload as RuntimeInstallProgress);
  });

  await listen("player_closed", () => {
    state.playLaunching = false;
    resetPlayButtons();
  });

  await listen("playback_finished", async (event: any) => {
    const { epNum, percent, timePos, elapsed } = event.payload;

    // ── Local history (works without AniList login) ────────────────────────────
    const histMedia = state.sidebarItems.find(m => m.id === state.activePlayingAnimeId) ?? state.selectedMedia;
    if (histMedia && state.activePlayingEp != null) {
      const nowSec = Math.floor(Date.now() / 1000);
      try {
        await invoke("append_history", {
          entry: {
            animeId:    histMedia.id,
            animeTitle: histMedia.title.english || histMedia.title.romaji,
            cover:      histMedia.coverImage.medium,
            epNum:      state.activePlayingEp,
            watchedAt:  nowSec,
          }
        });
      } catch { /* non-critical, ignore */ }
    }

    if (!state.config.anilist_token) return;

    // Don't sync if the player was open for less than 60 seconds — prevents a
    // false trigger when the user skips to the outro immediately after opening.
    if ((elapsed ?? 0) < 60) return;

    // Capture IDs before any awaits — user may navigate during the AniSkip fetch.
    const capturedAnimeId = state.activePlayingAnimeId;
    const capturedEp = state.activePlayingEp;

    let isFinished = percent > 0.85;

    const playedMedia = state.sidebarItems.find(m => m.id === capturedAnimeId) ?? state.selectedMedia;
    if (playedMedia?.idMal && timePos > 0) {
      try {
        const res = await fetch(`https://api.aniskip.com/v2/skip-times/${playedMedia.idMal}/${epNum}?types=ed&episodeLength=0`);
        if (res.ok) {
          const data = await res.json();
          const ed = data.results?.find((r: any) => r.skipType === "ed");
          if (ed?.interval?.startTime) {
            isFinished = timePos >= (ed.interval.startTime - 10);
          }
        }
      } catch (err) {
        console.error("AniSkip fetch failed", err);
      }
    }

    if (isFinished) {
      if (state.config.auto_sync) {
        try {
          await invoke("sync_progress", { mediaId: capturedAnimeId, epNum: capturedEp });
          toast(`Auto-synced Episode ${capturedEp}`, "success");
        } catch (err: any) {
          toast(`Failed to auto-sync: ${err}`, "error");
        }
      } else if (state.config.confirm_before_sync) {
        state.pendingSyncEp = epNum;
        state.pendingSyncAnimeId = capturedAnimeId;
        showSyncBar(epNum);
      }
    }
  });

  await listen("download_chunk", (event: any) => {
    const log = document.getElementById("download-log");
    if (log) {
      state.downloadLogBuffer += event.payload.chunk;
      const clean = state.downloadLogBuffer.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, "");
      const finalLines = clean.split('\n').map(l => {
        const parts = l.split('\r');
        return parts[parts.length - 1];
      });
      log.innerHTML = "";
      for (const fl of finalLines) {
        if (fl.trim() !== "") {
          const div = el("div");
          div.textContent = fl;
          log.appendChild(div);
        }
      }
      log.scrollTop = log.scrollHeight;
    }
  });

  await listen("download_finished", (event: any) => {
    const status = document.getElementById("download-status");
    if (status) {
      status.textContent = event.payload.success ? "✓ Download complete!" : "✗ Download failed.";
      status.style.color = event.payload.success ? "var(--green)" : "var(--red)";
    }
    // Remove progress bar
    document.querySelector(".download-progress")?.remove();
    toast(
      event.payload.success ? "Download complete!" : "Download failed.",
      event.payload.success ? "success" : "error"
    );
  });
}

// ─── Boot ─────────────────────────────────────────────────────────────────────

window.addEventListener("DOMContentLoaded", init);

// ─── Dev-only Sync Test Helper ────────────────────────────────────────────────
// Stripped from production builds via import.meta.env.DEV.
// Usage: __testSync(), __testSync(5), __testSync(5, 0.92, 1335)

if (import.meta.env.DEV) {
  (window as any).__testSync = async (overrideEp?: number, overridePercent = 0.92, overrideTimePos = 1300) => {
    if (!state.selectedMedia) {
      console.warn("[testSync] No anime selected. Click one in the sidebar first.");
      return;
    }
    const ep = overrideEp ?? (state.selectedMedia.mediaListEntry?.progress ?? 0) + 1;
    console.info("[testSync] Simulating playback_finished →", { epNum: ep, percent: overridePercent, timePos: overrideTimePos, elapsed: 9999 }, "for:", state.selectedMedia.title.romaji);
    state.activePlayingAnimeId = state.selectedMedia.id;
    state.activePlayingEp = ep;

    let isFinished = overridePercent > 0.85;
    const playedMedia = state.sidebarItems.find(m => m.id === state.selectedMedia!.id) ?? state.selectedMedia;
    if (playedMedia?.idMal && overrideTimePos > 0) {
      try {
        const res = await fetch(`https://api.aniskip.com/v2/skip-times/${playedMedia.idMal}/${ep}?types=ed&episodeLength=0`);
        if (res.ok) {
          const data = await res.json();
          const ed = data.results?.find((r: any) => r.skipType === "ed");
          if (ed?.interval?.startTime) {
            isFinished = overrideTimePos >= (ed.interval.startTime - 10);
            console.info(`[testSync] AniSkip ED starts at ${ed.interval.startTime}s → isFinished = ${isFinished}`);
          } else {
            console.info("[testSync] AniSkip returned no ED data, falling back to percent check.");
          }
        }
      } catch (err) {
        console.warn("[testSync] AniSkip fetch failed:", err);
      }
    }

    if (isFinished) {
      if (state.config.auto_sync) {
        try {
          await invoke("sync_progress", { mediaId: state.selectedMedia!.id, epNum: ep });
          toast(`[DEV] Auto-synced Episode ${ep}`, "success");
          console.info("[testSync] Auto-sync fired successfully.");
        } catch (err) {
          toast(`[DEV] Auto-sync failed: ${err}`, "error");
        }
      } else if (state.config.confirm_before_sync) {
        state.pendingSyncEp = ep;
        state.pendingSyncAnimeId = state.selectedMedia!.id;
        showSyncBar(ep);
        console.info("[testSync] Sync confirm bar shown. Click 'Yes' to complete.");
      }
    } else {
      console.info(`[testSync] Not finished (percent=${overridePercent}). Try __testSync(ep, 0.92, 1335).`);
      toast(`[DEV] Not finished — percent=${(overridePercent * 100).toFixed(0)}%`, "info");
    }
  };
  console.info("%c[AniGUI Dev] __testSync() available.", "color: #a855f7; font-weight: bold;");
}
