// ─── AniGUI – Boot & Event Wiring ─────────────────────────────────────────────
// This file is intentionally slim. All view logic lives in src/views/,
// all components in src/components/, and shared state in src/state.ts.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Config, RuntimeInstallProgress, RuntimeStatus } from "./types";
import { state } from "./state";

// Views
import { loadBrowse } from "./views/browse";
import { loadDownloads } from "./views/downloads";
import { loadHome, loadSearchResults, jumpToWatching } from "./views/home";
import { applySyncedProgress, resetPlayButtons, setPlayingEpisode } from "./views/detail";
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
import { handleDownloadChunk, handleDownloadFinished } from "./components/download-queue";

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

  // ── Initial screen ───────────────────────────────────────────────────────────
  loadHome();

  // ── Rail navigation ──────────────────────────────────────────────────────────
  document.getElementById("btn-home")!.addEventListener("click", loadHome);
  document.getElementById("btn-browse")!.addEventListener("click", loadBrowse);
  document.getElementById("btn-watching")!.addEventListener("click", jumpToWatching);
  document.getElementById("btn-downloads")!.addEventListener("click", loadDownloads);
  document.getElementById("btn-calendar")!.addEventListener("click", loadCalendar);
  document.getElementById("btn-shortcuts")!.addEventListener("click", toggleOverlay);

  // ── Profile (login-status) ────────────────────────────────────────────────────
  // Logged in  → open Profile on Stats tab
  // Not logged in → open Profile on History tab (history works without login)
  document.getElementById("login-status")!.addEventListener("click", () => {
    loadProfile(state.config.anilist_token ? "stats" : "history");
  });

  // ── Search ───────────────────────────────────────────────────────────────────
  let searchTimer: ReturnType<typeof setTimeout>;
  document.getElementById("search-input")!.addEventListener("input", (e) => {
    const q = (e.target as HTMLInputElement).value.trim();
    clearTimeout(searchTimer);
    if (q.length < 2) { if (!q) loadHome(); return; }
    searchTimer = setTimeout(() => loadSearchResults(q), 400);
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

  await listen("playback_episode_changed", (event: any) => {
    setPlayingEpisode(event.payload.epNum);
  });

  await listen("playback_finished", async (event: any) => {
    const { animeId, epNum, percent, timePos, advanced } = event.payload;

    // animeId/epNum come from the payload, not state.activePlayingAnimeId/Ep:
    // player_closed (emitted first) resets those, and the player's next-episode
    // button moves the episode on before this handler gets to run.
    const playedMedia =
      state.sidebarItems.find(m => m.id === animeId) ??
      (state.selectedMedia?.id === animeId ? state.selectedMedia : undefined);

    // ── Local history (works without AniList login) ────────────────────────────
    if (playedMedia) {
      const nowSec = Math.floor(Date.now() / 1000);
      try {
        await invoke("append_history", {
          entry: {
            animeId:    playedMedia.id,
            animeTitle: playedMedia.title.english || playedMedia.title.romaji,
            cover:      playedMedia.coverImage.medium,
            epNum,
            watchedAt:  nowSec,
          }
        });
      } catch { /* non-critical, ignore */ }
    }

    if (!state.config.anilist_token) return;

    if (animeId == null) return;

    const capturedAnimeId = animeId;
    const capturedEp = epNum;

    // Watched = the user asked for the next episode (`advanced`), or got near the
    // outro. Time spent in the player doesn't matter: opening an episode and
    // jumping to the end counts, opening it and closing it early doesn't.
    let isFinished = advanced || percent > 0.85;

    if (!advanced && playedMedia?.idMal && timePos > 0) {
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
          applySyncedProgress(capturedAnimeId, capturedEp);
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
    handleDownloadChunk(event.payload.chunk);
  });

  await listen("download_finished", (event: any) => {
    handleDownloadFinished(!!event.payload.success);
  });
}

// ─── Lock Down Production Builds ──────────────────────────────────────────────
// Blocks the right-click context menu and the common view-source/devtools
// shortcuts in shipped builds, so the app doesn't invite poking at internals.
// Left alone in dev (import.meta.env.DEV) so debugging still works normally.
// This is a UX deterrent, not a security boundary — a determined user can
// still reach devtools another way.

if (!import.meta.env.DEV) {
  window.addEventListener("contextmenu", (e) => e.preventDefault());
  window.addEventListener("keydown", (e) => {
    const key = e.key.toLowerCase();
    const blockCombo =
      key === "f12" ||
      ((e.ctrlKey || e.metaKey) && e.shiftKey && ["i", "j", "c"].includes(key)) ||
      ((e.ctrlKey || e.metaKey) && ["u", "s"].includes(key));
    if (blockCombo) e.preventDefault();
  });
}

// ─── Boot ─────────────────────────────────────────────────────────────────────

window.addEventListener("DOMContentLoaded", init);

// ─── Dev-only Sync Test Helper ────────────────────────────────────────────────
// Stripped from production builds via import.meta.env.DEV.
// Usage: __testSync(), __testSync(5), __testSync(5, 0.92, 1335)

if (import.meta.env.DEV) {
  (window as any).__testSync = async (overrideEp?: number, overridePercent = 0.92, overrideTimePos = 1300, advanced = false) => {
    if (!state.selectedMedia) {
      console.warn("[testSync] No anime selected. Click one in the sidebar first.");
      return;
    }
    const ep = overrideEp ?? (state.selectedMedia.mediaListEntry?.progress ?? 0) + 1;
    console.info("[testSync] Simulating playback_finished →", { epNum: ep, percent: overridePercent, timePos: overrideTimePos, advanced }, "for:", state.selectedMedia.title.romaji);
    state.activePlayingAnimeId = state.selectedMedia.id;
    state.activePlayingEp = ep;

    let isFinished = advanced || overridePercent > 0.85;
    const playedMedia = state.sidebarItems.find(m => m.id === state.selectedMedia!.id) ?? state.selectedMedia;
    if (!advanced && playedMedia?.idMal && overrideTimePos > 0) {
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
          applySyncedProgress(state.selectedMedia!.id, ep);
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
