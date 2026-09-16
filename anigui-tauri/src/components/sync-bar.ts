// ─── Sync Confirm Bar ────────────────────────────────────────────────────────

import { invoke } from '@tauri-apps/api/core';
import { state } from '../state';
import { toast } from './toast';

export function showSyncBar(ep: number) {
  const bar = document.getElementById("sync-confirm")!;
  document.getElementById("sync-ep-label")!.textContent = String(ep);
  bar.classList.add("show");
}

export function hideSyncBar() {
  document.getElementById("sync-confirm")!.classList.remove("show");
}

export function wireSyncBar() {
  document.getElementById("sync-yes")!.addEventListener("click", async () => {
    if (!state.pendingSyncAnimeId || !state.pendingSyncEp) return;
    const syncAnimeId = state.pendingSyncAnimeId;
    const syncEp = state.pendingSyncEp;
    hideSyncBar();
    state.pendingSyncEp = null;
    state.pendingSyncAnimeId = null;
    try {
      await invoke("sync_progress", { mediaId: syncAnimeId, epNum: syncEp });
      // Update local progress only if the user is still viewing the same anime
      if (state.selectedMedia?.id === syncAnimeId) {
        if (state.selectedMedia.mediaListEntry) {
          state.selectedMedia.mediaListEntry.progress = syncEp;
        } else {
          state.selectedMedia.mediaListEntry = { id: 0, progress: syncEp, status: "CURRENT" };
        }
        // Re-render detail to reflect new progress
        const { renderDetail } = await import('../views/detail');
        renderDetail();
      }
      toast(`Synced EP ${syncEp}!`, "success");
    } catch (e: any) {
      toast("Sync failed: " + e, "error");
    }
  });

  document.getElementById("sync-no")!.addEventListener("click", () => {
    hideSyncBar();
    state.pendingSyncEp = null;
    state.pendingSyncAnimeId = null;
  });
}
