// ─── AniGUI Shared Mutable State ─────────────────────────────────────────────
// All view modules read and write this single object.
// Keeping it in one place makes the data flow easy to trace.

import type { Media, Config, TabName } from './types';

export const state = {
  config: {
    bash_path: "",
    quality: "best",
    confirm_before_sync: true,
    anilist_token: "",
    download_dir: "",
    theme: "purple",
    auto_sync: false,
    skip_auto_setup: false,
  } as Config,

  currentTab: "trending" as TabName,

  // Sidebar
  sidebarItems:   [] as Media[],
  sidebarPage:    1,
  sidebarHasMore: false,
  sidebarLoading: false,

  // Detail panel
  selectedMedia: null as Media | null,
  selectedEp:    null as number | null,

  // Sync confirm
  pendingSyncEp:      null as number | null,
  pendingSyncAnimeId: null as number | null,

  // Auth
  viewerName: null as string | null,

  // Playback
  playLaunching:       false,
  activePlayingAnimeId: null as number | null,
  activePlayingEp:      null as number | null,

  // Downloads
  downloadLogBuffer: "",
};
