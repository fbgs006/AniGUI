// ─── Auto-Updater ─────────────────────────────────────────────────────────────
// Checks GitHub Releases on startup (non-blocking).
// Shows a non-intrusive banner in the header if an update is available.
// Only runs in production builds — Tauri skips update checks in dev mode.

import { invoke } from '@tauri-apps/api/core';

export async function checkForUpdate() {
  // In dev mode the updater is a no-op — skip silently.
  if (import.meta.env.DEV) return;

  try {
    const result = await invoke<{ available: boolean; version?: string } | null>('check_for_update');
    if (result?.available && result.version) {
      showUpdateBanner(result.version);
    }
  } catch {
    // Silently ignore — network errors, no update endpoint yet, etc.
  }
}

function showUpdateBanner(version: string) {
  const banner = document.getElementById('update-banner');
  const label  = document.getElementById('update-version-label');
  if (!banner || !label) return;

  label.textContent = `v${version} is available`;
  banner.classList.add('show');

  document.getElementById('update-install-btn')?.addEventListener('click', async () => {
    const btn = document.getElementById('update-install-btn') as HTMLButtonElement;
    btn.textContent = 'Updating…';
    btn.disabled = true;
    try {
      await invoke('install_update');
      // App will relaunch automatically after install
    } catch (e) {
      btn.textContent = 'Update Now';
      btn.disabled = false;
      console.error('[updater] install failed:', e);
    }
  });

  document.getElementById('update-dismiss-btn')?.addEventListener('click', () => {
    banner.classList.remove('show');
  });
}
