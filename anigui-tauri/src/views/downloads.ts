// ─── Downloads View ───────────────────────────────────────────────────────────
import './downloads.css';

import { invoke } from '@tauri-apps/api/core';
import { state } from '../state';
import { setActiveNav } from '../utils';
import { toast } from '../components/toast';
import { renderQueuePanel } from '../components/download-queue';

function formatSize(bytes: number): string {
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(0) + ' KB';
  if (bytes < 1024 * 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
  return (bytes / (1024 * 1024 * 1024)).toFixed(2) + ' GB';
}

export async function loadDownloads() {
  state.currentTab = "downloads";
  setActiveNav("btn-downloads");

  const main = document.getElementById("main-panel")!;
  main.innerHTML = `<div id="download-queue-panel" class="downloads-container"></div><div class="downloads-empty"><h2>⬇ Downloads</h2><p>Loading your downloaded episodes...</p></div>`;
  renderQueuePanel();

  try {
    const files = await invoke<any[]>("get_downloads");

    if (!files || !files.length) {
      main.innerHTML = `
        <div id="download-queue-panel" class="downloads-container"></div>
        <div class="downloads-empty fade-in">
          <svg width="56" height="56" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
            <polyline points="7 10 12 15 17 10"/>
            <line x1="12" y1="15" x2="12" y2="3"/>
          </svg>
          <h2>No Downloads Yet</h2>
          <p>Episodes you download will appear here.<br>Open an anime's detail page and hit <strong>⬇ Download</strong> on any episode.</p>
        </div>`;
      renderQueuePanel();
      return;
    }

    // Group files by Anime Title
    const groups: Record<string, any[]> = {};
    let totalSize = 0;
    for (const f of files) {
      totalSize += f.size || 0;
      let animeName = "Unknown Anime";
      let epNum: string | number = "?";
      const match = f.name.match(/^(.*?)[\s_]+Episode[\s_]+(\d+)/i);
      if (match) {
        animeName = match[1].replace(/_/g, " ").trim();
        epNum = match[2];
      } else {
        animeName = f.name.replace(/\.(mp4|mkv)$/i, "");
      }
      if (!groups[animeName]) groups[animeName] = [];
      f.epNum = epNum;
      groups[animeName].push(f);
    }

    let html = `<div id="download-queue-panel" class="downloads-container"></div>
    <div class="downloads-container fade-in">
      <div class="downloads-header">
        <h2>Offline Downloads</h2>
        <span class="downloads-summary">${files.length} file${files.length !== 1 ? 's' : ''} · ${formatSize(totalSize)}</span>
      </div>`;
    for (const [anime, eps] of Object.entries(groups)) {
      eps.sort((a, b) => (parseInt(a.epNum) || 0) - (parseInt(b.epNum) || 0));
      html += `<div class="download-group">
        <div class="download-group-title" data-search-title="${anime.replace(/"/g, '&quot;')}">${anime}</div>
        <div class="download-items">`;
      for (const ep of eps) {
        html += `
          <div class="download-item">
            <div class="download-info" style="display:flex;align-items:center;">
              <span class="download-ep-num">Episode ${ep.epNum}</span>
              <span class="download-size">${formatSize(ep.size)}</span>
            </div>
            <div class="download-actions">
              <button class="btn btn-primary btn-play-dl" data-path="${ep.path}">▶ Play</button>
              <button class="btn btn-outline btn-del-dl" data-path="${ep.path}">🗑 Delete</button>
            </div>
          </div>`;
      }
      html += `</div></div>`;
    }
    html += `</div>`;
    main.innerHTML = html;
    renderQueuePanel();

    // ── Wire events via delegation (no globals) ───────────────────────────────
    main.querySelectorAll<HTMLElement>(".download-group-title").forEach(title => {
      title.addEventListener("click", () => searchAndLoad(title.dataset.searchTitle ?? ""));
    });

    main.querySelectorAll(".btn-play-dl").forEach(b => b.addEventListener("click", async (e) => {
      const path = (e.currentTarget as HTMLElement).dataset.path!;
      await invoke("play_local_file", { path });
    }));

    main.querySelectorAll(".btn-del-dl").forEach(b => b.addEventListener("click", async (e) => {
      const path = (e.currentTarget as HTMLElement).dataset.path!;
      const btn = e.currentTarget as HTMLButtonElement;
      btn.disabled = true;
      btn.textContent = "Deleting...";
      try {
        await invoke("delete_local_file", { path });
        loadDownloads();
      } catch (err) {
        toast(`Failed to delete: ${err}`, "error");
        btn.disabled = false;
        btn.textContent = "🗑 Delete";
      }
    }));

  } catch (err) {
    main.innerHTML = `<div class="welcome"><p style="color:var(--red)">Failed to load downloads: ${err}</p></div>`;
  }
}

// ─── Search helper (replaces old global) ──────────────────────────────────────

async function searchAndLoad(title: string) {
  const main = document.getElementById("main-panel")!;
  main.innerHTML = `<div class="browse-loading"><div class="spinner"></div><div>Searching...</div></div>`;
  try {
    const result = await invoke<any>("advanced_search", { search: title });
    if (result?.data?.Page?.media?.length > 0) {
      const { selectMedia } = await import('./detail');
      selectMedia(result.data.Page.media[0]);
    } else {
      main.innerHTML = `<div class="downloads-empty"><h2>Not Found</h2><p>Could not find ${title} on AniList.</p></div>`;
    }
  } catch (e) {
    console.error(e);
  }
}
