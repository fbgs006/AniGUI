// ─── Settings Modal ──────────────────────────────────────────────────────────

import { invoke } from '@tauri-apps/api/core';
import { open as dialogOpen } from '@tauri-apps/plugin-dialog';
import { state } from '../state';
import { toast } from './toast';

export function openSettings() {
  (document.getElementById("s-token") as HTMLInputElement).value = state.config.anilist_token || "";
  (document.getElementById("s-bash") as HTMLInputElement).value = state.config.bash_path || "";
  (document.getElementById("s-dldir") as HTMLInputElement).value = state.config.download_dir || "";
  (document.getElementById("s-theme") as HTMLSelectElement).value = state.config.theme || "purple";
  (document.getElementById("s-quality") as HTMLSelectElement).value = state.config.quality || "best";
  (document.getElementById("s-autosync") as HTMLInputElement).checked = state.config.auto_sync || false;
  (document.getElementById("s-dub") as HTMLInputElement).checked = state.config.dub || false;
  document.getElementById("modal-settings")!.classList.add("open");
}

export async function saveSettings() {
  state.config.anilist_token = (document.getElementById("s-token") as HTMLInputElement).value.trim();
  state.config.bash_path     = (document.getElementById("s-bash") as HTMLInputElement).value.trim();
  state.config.download_dir  = (document.getElementById("s-dldir") as HTMLInputElement).value.trim();
  state.config.theme         = (document.getElementById("s-theme") as HTMLSelectElement).value;
  state.config.quality       = (document.getElementById("s-quality") as HTMLSelectElement).value;
  state.config.auto_sync     = (document.getElementById("s-autosync") as HTMLInputElement).checked;
  state.config.dub           = (document.getElementById("s-dub") as HTMLInputElement).checked;

  await invoke("save_config", { config: state.config });

  // Apply theme immediately
  document.body.setAttribute("data-theme", state.config.theme);

  state.viewerName = null;
  updateLoginStatus();
  await fetchViewerName();
  document.getElementById("modal-settings")!.classList.remove("open");
  toast("Settings saved!", "success");

  // Reload current tab to apply token changes
  const { loadTab } = await import('../views/sidebar');
  loadTab(state.currentTab);
}

export function updateLoginStatus() {
  const statusEl = document.getElementById("login-status");
  const label    = document.getElementById("login-label");
  if (statusEl && label) {
    if (state.config.anilist_token) {
      statusEl.classList.add("logged-in");
      label.textContent = state.viewerName ?? "Logged in";
    } else {
      statusEl.classList.remove("logged-in");
      state.viewerName = null;
      label.textContent = "Not logged in";
    }
  }
}

export async function fetchViewerName() {
  if (!state.config.anilist_token) return;
  try {
    const data = await invoke<any>("get_viewer_info");
    state.viewerName = data?.data?.Viewer?.name ?? null;
    updateLoginStatus();
  } catch { /* not logged in or network error */ }
}

export function wireSettings() {
  document.getElementById("btn-settings")!.addEventListener("click", openSettings);
  document.getElementById("close-settings")!.addEventListener("click", () =>
    document.getElementById("modal-settings")!.classList.remove("open")
  );
  document.getElementById("cancel-settings")!.addEventListener("click", () =>
    document.getElementById("modal-settings")!.classList.remove("open")
  );
  document.getElementById("save-settings")!.addEventListener("click", saveSettings);
  document.getElementById("s-open-anilist")!.addEventListener("click", () =>
    invoke("open_anilist_login")
  );
  document.getElementById("s-browse")!.addEventListener("click", async () => {
    try {
      const dir = await dialogOpen({ directory: true, multiple: false }) as string | null;
      if (dir) (document.getElementById("s-dldir") as HTMLInputElement).value = dir;
    } catch { /* dialog plugin not available */ }
  });
  // Note: login-status click is handled in main.ts (opens Profile panel)
  document.getElementById("modal-settings")!.addEventListener("click", (e) => {
    if (e.target === document.getElementById("modal-settings")) {
      document.getElementById("modal-settings")!.classList.remove("open");
    }
  });
}
