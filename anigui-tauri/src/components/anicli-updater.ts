// ─── Ani-CLI Update Checker ───────────────────────────────────────────────────
// Checks if the installed ani-cli is up-to-date on startup (non-blocking).
// Shows a warning banner if an update is available, since outdated ani-cli
// can cause playback failures. The user can update directly from the banner.

import { invoke } from '@tauri-apps/api/core';
import { toast } from './toast';

interface AniCliVersionResult {
  installed: boolean;
  local_version: string | null;
  latest_version: string | null;
  update_available: boolean;
}

interface AniCliUpdateResult {
  success: boolean;
  output: string;
  stderr: string;
}

export async function checkAniCliVersion() {
  try {
    const result = await invoke<AniCliVersionResult>('check_anicli_version');

    if (!result.installed) {
      showAniCliBanner(null, null, false);
      return;
    }

    if (result.update_available && result.latest_version) {
      showAniCliBanner(result.local_version, result.latest_version, true);
    }
  } catch {
    // Silently ignore — bash not found, network error, etc.
  }
}

function showAniCliBanner(
  localVersion: string | null,
  latestVersion: string | null,
  isUpdate: boolean
) {
  const banner = document.getElementById('anicli-update-banner');
  const label  = document.getElementById('anicli-update-label');
  if (!banner || !label) return;

  if (!isUpdate) {
    label.textContent = 'ani-cli is not installed — playback will not work';
  } else {
    label.textContent = `ani-cli ${localVersion} → ${latestVersion} available`;
  }

  banner.classList.add('show');

  const updateBtn = document.getElementById('anicli-update-btn');
  const dismissBtn = document.getElementById('anicli-dismiss-btn');

  updateBtn?.addEventListener('click', async () => {
    const btn = updateBtn as HTMLButtonElement;
    btn.textContent = 'Updating…';
    btn.disabled = true;
    try {
      const result = await invoke<AniCliUpdateResult>('update_anicli');
      if (result.success) {
        toast('ani-cli updated successfully!', 'success');
        banner.classList.remove('show');
      } else {
        toast('ani-cli update failed. Try running ani-cli -U manually.', 'error');
        btn.textContent = 'Update';
        btn.disabled = false;
      }
    } catch (e) {
      toast(`Update failed: ${e}`, 'error');
      btn.textContent = 'Update';
      btn.disabled = false;
    }
  });

  dismissBtn?.addEventListener('click', () => {
    banner.classList.remove('show');
  });
}
