// ─── Profile Panel — Stats + History ─────────────────────────────────────────
import './profile.css';

import { invoke } from '@tauri-apps/api/core';
import { save as saveDialog } from '@tauri-apps/plugin-dialog';
import { state } from '../state';
import { toast } from '../components/toast';
import { setActiveNav, wireDropdown } from '../utils';

type ProfileTab = 'stats' | 'history' | 'manage';
let _activeTab: ProfileTab = 'stats';

// ─── Entry Point ──────────────────────────────────────────────────────────────

export async function loadProfile(tab?: ProfileTab) {
  _activeTab = tab ?? (state.config.anilist_token ? 'stats' : 'history');
  setActiveNav('login-status');

  const main = document.getElementById('main-panel')!;
  main.innerHTML = renderShell();

  // Wire tab switcher
  document.getElementById('profile-tab-stats')!.addEventListener('click', () => switchTab('stats'));
  document.getElementById('profile-tab-history')!.addEventListener('click', () => switchTab('history'));
  document.getElementById('profile-tab-manage')!.addEventListener('click', () => switchTab('manage'));

  // Set initial active tab button
  document.getElementById(`profile-tab-${_activeTab}`)!.classList.add('active');

  // Hydrate profile header & content
  await Promise.all([hydrateHeader(), loadContent(_activeTab)]);
}

// ─── Shell HTML ───────────────────────────────────────────────────────────────

function renderShell(): string {
  return `
    <div class="profile-panel fade-in">
      <div class="profile-header">
        <div class="profile-avatar-wrap" id="profile-avatar-wrap">
          <div class="profile-avatar skeleton"></div>
        </div>
        <div class="profile-meta" id="profile-meta">
          <div class="skeleton" style="height:18px;width:130px;border-radius:4px;margin-bottom:8px;"></div>
          <div class="skeleton" style="height:12px;width:80px;border-radius:4px;"></div>
        </div>
        <button class="profile-settings-btn btn btn-outline" id="profile-open-settings" title="Open Settings">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
          Settings
        </button>
      </div>

      <div class="profile-tabs">
        <button class="profile-tab-btn" id="profile-tab-stats" data-tab="stats">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="18" y="3" width="4" height="18"/><rect x="10" y="8" width="4" height="13"/><rect x="2" y="13" width="4" height="8"/></svg>
          Stats
        </button>
        <button class="profile-tab-btn" id="profile-tab-history" data-tab="history">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
          History
        </button>
        <button class="profile-tab-btn" id="profile-tab-manage" data-tab="manage">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 11l3 3L22 4"/><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/></svg>
          Manage
        </button>
      </div>

      <div class="profile-content" id="profile-content">
        <div class="profile-loading">
          <div class="skeleton" style="height:100px;border-radius:var(--radius);margin-bottom:16px;"></div>
          <div class="skeleton" style="height:160px;border-radius:var(--radius);margin-bottom:16px;"></div>
          <div class="skeleton" style="height:120px;border-radius:var(--radius);"></div>
        </div>
      </div>
    </div>
  `;
}

// ─── Tab Switching ────────────────────────────────────────────────────────────

async function switchTab(tab: ProfileTab) {
  if (_activeTab === tab) return;
  _activeTab = tab;
  document.querySelectorAll('.profile-tab-btn').forEach(b => b.classList.remove('active'));
  document.getElementById(`profile-tab-${tab}`)?.classList.add('active');
  await loadContent(tab);
}

async function loadContent(tab: ProfileTab) {
  const content = document.getElementById('profile-content')!;
  content.innerHTML = `<div class="profile-loading">
    <div class="skeleton" style="height:100px;border-radius:var(--radius);margin-bottom:16px;"></div>
    <div class="skeleton" style="height:160px;border-radius:var(--radius);margin-bottom:16px;"></div>
    <div class="skeleton" style="height:120px;border-radius:var(--radius);"></div>
  </div>`;
  if (tab === 'stats') await renderStats();
  else if (tab === 'manage') await renderManage();
  else await renderHistory();
}

// ─── Profile Header Hydration ─────────────────────────────────────────────────

async function hydrateHeader() {
  const avatarWrap = document.getElementById('profile-avatar-wrap');
  const meta       = document.getElementById('profile-meta');
  const settingsBtn = document.getElementById('profile-open-settings');

  settingsBtn?.addEventListener('click', async () => {
    const { openSettings } = await import('../components/settings');
    openSettings();
  });

  if (!state.config.anilist_token) {
    if (avatarWrap) avatarWrap.innerHTML = `
      <div class="profile-avatar profile-avatar--anon">
        <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>
      </div>`;
    if (meta) meta.innerHTML = `
      <div class="profile-name" style="color:var(--text3)">Not logged in</div>
      <div class="profile-sub">Connect AniList to sync stats</div>`;
    return;
  }

  try {
    const data = await invoke<any>('get_viewer_info');
    const viewer = data?.data?.Viewer;
    if (!viewer) return;
    if (avatarWrap) avatarWrap.innerHTML = viewer.avatar?.medium
      ? `<img class="profile-avatar" src="${viewer.avatar.medium}" alt="${viewer.name}" onerror="this.style.opacity=0.3"/>`
      : `<div class="profile-avatar profile-avatar--anon"><svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg></div>`;
    if (meta) meta.innerHTML = `
      <div class="profile-name">${esc(viewer.name)}</div>
      <div class="profile-sub">AniList · <a class="profile-anilist-link" href="https://anilist.co/user/${esc(viewer.name)}" target="_blank">${esc(viewer.name)} ↗</a></div>`;
  } catch { /* silently fail */ }
}

// ─── Stats Tab ────────────────────────────────────────────────────────────────

async function renderStats() {
  const content = document.getElementById('profile-content')!;

  if (!state.config.anilist_token) {
    content.innerHTML = `
      <div class="profile-empty fade-in">
        <svg width="52" height="52" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2"><rect x="18" y="3" width="4" height="18"/><rect x="10" y="8" width="4" height="13"/><rect x="2" y="13" width="4" height="8"/></svg>
        <h3>Connect AniList to see your stats</h3>
        <p>Log in with your AniList account to track your full anime journey — watch time, scores, genres and more.</p>
        <button class="btn btn-primary" id="stats-open-settings">Open Settings</button>
      </div>`;
    document.getElementById('stats-open-settings')?.addEventListener('click', async () => {
      const { openSettings } = await import('../components/settings');
      openSettings();
    });
    return;
  }

  try {
    const data = await invoke<any>('get_user_stats');
    const s = data?.data?.User?.statistics?.anime;
    if (!s) throw new Error('No stats data returned');

    const genres:  { genre: string; count: number }[] = s.genres  ?? [];
    const formats: { format: string; count: number; minutesWatched: number }[] = s.formats ?? [];
    const statuses:{ status: string; count: number }[] = s.statuses ?? [];
    const hours   = Math.floor((s.minutesWatched ?? 0) / 60);
    const mins    = (s.minutesWatched ?? 0) % 60;
    const maxG    = genres[0]?.count || 1;
    const score   = s.meanScore as number | undefined;

    const STATUS_LABEL: Record<string, string> = {
      CURRENT: 'Watching', COMPLETED: 'Completed', PAUSED: 'Paused',
      DROPPED: 'Dropped', PLANNING: 'Planning', REPEATING: 'Rewatching',
    };

    content.innerHTML = `
      <div class="stats-body fade-in">

        <!-- ── Stat Cards ──────────────────────────────────────────────────── -->
        <div class="stat-cards">
          <div class="stat-card">
            <div class="stat-value">${(s.count ?? 0).toLocaleString()}</div>
            <div class="stat-label">Anime</div>
          </div>
          <div class="stat-card">
            <div class="stat-value">${(s.episodesWatched ?? 0).toLocaleString()}</div>
            <div class="stat-label">Episodes</div>
          </div>
          <div class="stat-card">
            <div class="stat-value">${hours.toLocaleString()}<span class="stat-unit">h</span> ${mins}<span class="stat-unit">m</span></div>
            <div class="stat-label">Watch Time</div>
          </div>
          <div class="stat-card${score ? (score >= 75 ? ' stat-card--green' : score >= 55 ? ' stat-card--yellow' : ' stat-card--red') : ''}">
            <div class="stat-value">${score?.toFixed(1) ?? '—'}</div>
            <div class="stat-label">Mean Score</div>
          </div>
        </div>

        <!-- ── Top Genres ──────────────────────────────────────────────────── -->
        ${genres.length ? `
        <div class="stats-section">
          <div class="stats-section-title">Top Genres</div>
          <div class="genre-bars">
            ${genres.map(g => `
              <div class="genre-bar-row">
                <div class="genre-bar-label">${esc(g.genre)}</div>
                <div class="genre-bar-track">
                  <div class="genre-bar-fill" style="width:${Math.round((g.count / maxG) * 100)}%"></div>
                </div>
                <div class="genre-bar-count">${g.count}</div>
              </div>`).join('')}
          </div>
        </div>` : ''}

        <!-- ── Format Breakdown ────────────────────────────────────────────── -->
        ${formats.length ? `
        <div class="stats-section">
          <div class="stats-section-title">By Format</div>
          <div class="format-pills">
            ${formats.map(f => `
              <div class="format-pill">
                <span class="format-pill-count">${f.count}</span>
                <span class="format-pill-label">${f.format.replace(/_/g,' ')}</span>
              </div>`).join('')}
          </div>
        </div>` : ''}

        <!-- ── Status Breakdown ────────────────────────────────────────────── -->
        ${statuses.length ? `
        <div class="stats-section">
          <div class="stats-section-title">By Status</div>
          <div class="status-breakdown">
            ${statuses.map(st => `
              <div class="status-row">
                <span class="status-dot status-dot--${st.status.toLowerCase()}"></span>
                <span class="status-row-label">${STATUS_LABEL[st.status] ?? st.status}</span>
                <span class="status-row-count">${st.count}</span>
              </div>`).join('')}
          </div>
        </div>` : ''}

      </div>`;
  } catch (e: any) {
    content.innerHTML = `<div class="profile-empty fade-in">
      <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
      <h3>Failed to load stats</h3>
      <p style="color:var(--red);font-size:12px;">${esc(String(e))}</p>
    </div>`;
  }
}

// ─── History Tab ──────────────────────────────────────────────────────────────

// Anime episodes run ~24 minutes on average — used only to turn an episode
// count into an approximate watch-time figure since history.json doesn't
// record per-episode runtime.
const APPROX_EP_MINUTES = 24;

async function renderHistory() {
  const content = document.getElementById('profile-content')!;
  try {
    const entries = await invoke<any[]>('get_history');

    if (!entries.length) {
      content.innerHTML = `
        <div class="profile-empty fade-in">
          <svg width="52" height="52" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
          <h3>No watch history yet</h3>
          <p>Play an episode and it'll appear here, grouped by date.</p>
        </div>`;
      return;
    }

    const grouped = groupByDate(entries);
    const totalMinutes = entries.length * APPROX_EP_MINUTES;
    const hours = Math.floor(totalMinutes / 60);
    const mins = totalMinutes % 60;

    const byShow = new Map<number, { title: string; count: number }>();
    for (const e of entries) {
      const cur = byShow.get(e.animeId);
      if (cur) cur.count++;
      else byShow.set(e.animeId, { title: e.animeTitle, count: 1 });
    }
    const topShow = [...byShow.values()].sort((a, b) => b.count - a.count)[0];

    let html = `<div class="history-body fade-in">
      <div class="history-stats-bar">
        <div class="history-stat"><div class="history-stat-value">${entries.length}</div><div class="history-stat-label">Episodes</div></div>
        <div class="history-stat"><div class="history-stat-value">${hours}<span class="stat-unit">h</span> ${mins}<span class="stat-unit">m</span></div><div class="history-stat-label">Approx. watch time</div></div>
        <div class="history-stat"><div class="history-stat-value">${byShow.size}</div><div class="history-stat-label">Distinct shows</div></div>
        ${topShow ? `<div class="history-stat"><div class="history-stat-value" title="${esc(topShow.title)}">${topShow.count}×</div><div class="history-stat-label">Most rewatched — ${esc(topShow.title)}</div></div>` : ''}
      </div>
      <div class="history-genre-section" id="history-genres"></div>
      <div class="history-top-bar">
        <span class="history-count">${entries.length} episode${entries.length !== 1 ? 's' : ''} watched · last ${entries.length < 200 ? entries.length : '200 (max kept)'}</span>
        <div style="display:flex;gap:8px;">
          <button class="btn btn-outline" id="export-history-btn" style="font-size:11px;padding:5px 12px;">⤓ Export CSV</button>
          <button class="btn btn-outline" id="clear-history-btn" style="font-size:11px;padding:5px 12px;">Clear All</button>
        </div>
      </div>`;

    for (const [label, dayEntries] of grouped) {
      html += `<div class="history-date-sep">${label}</div>`;
      for (const entry of dayEntries) {
        const t = new Date(entry.watchedAt * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
        html += `
          <div class="history-entry" data-title="${esc(entry.animeTitle)}" data-id="${entry.animeId}">
            <img class="history-cover" src="${entry.cover}" alt="" loading="lazy" onerror="this.style.opacity=0.3"/>
            <div class="history-info">
              <div class="history-title">${esc(entry.animeTitle)}</div>
              <div class="history-meta">Episode ${entry.epNum} · ${t}</div>
            </div>
            <svg class="history-arrow" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 18 15 12 9 6"/></svg>
          </div>`;
      }
    }
    html += '</div>';

    content.innerHTML = html;

    document.getElementById('clear-history-btn')?.addEventListener('click', async () => {
      await invoke('clear_history');
      toast('History cleared.', 'info');
      await renderHistory();
    });

    document.getElementById('export-history-btn')?.addEventListener('click', () => exportHistoryCsv(entries));

    content.querySelectorAll<HTMLElement>('.history-entry').forEach(card => {
      card.addEventListener('click', async () => {
        const title = card.dataset.title ?? '';
        if (!title) return;
        toast(`Loading ${title}…`, 'info');
        try {
          const res = await invoke<any>('search_anime', { query: title, page: 1 });
          const results = res?.data?.Page?.media ?? [];
          const id = Number(card.dataset.id);
          const match = results.find((r: any) => r.id === id) ?? results[0];
          if (match) {
            const { selectMedia } = await import('./detail');
            selectMedia(match);
          }
        } catch (e: any) { toast('Failed to load: ' + e, 'error'); }
      });
    });

    // Best-effort — hidden entirely if the genre lookup fails or there's no network.
    loadHistoryGenres([...byShow.keys()]);

  } catch (e: any) {
    content.innerHTML = `<div class="profile-empty fade-in">
      <h3>Failed to load history</h3>
      <p style="color:var(--red);font-size:12px;">${esc(String(e))}</p>
    </div>`;
  }
}

async function loadHistoryGenres(animeIds: number[]) {
  const host = document.getElementById('history-genres');
  if (!host || !animeIds.length) return;
  try {
    const res = await invoke<any>('get_media_genres', { ids: animeIds });
    const media: { id: number; genres: string[] }[] = res?.media ?? [];
    const counts = new Map<string, number>();
    for (const m of media) {
      for (const g of m.genres ?? []) counts.set(g, (counts.get(g) ?? 0) + 1);
    }
    const top = [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 6);
    if (!top.length) return;
    const max = top[0][1];
    host.innerHTML = `
      <div class="stats-section">
        <div class="stats-section-title">Top Genres — shows in your history</div>
        <div class="genre-bars">
          ${top.map(([genre, count]) => `
            <div class="genre-bar-row">
              <div class="genre-bar-label">${esc(genre)}</div>
              <div class="genre-bar-track"><div class="genre-bar-fill" style="width:${Math.round((count / max) * 100)}%"></div></div>
              <div class="genre-bar-count">${count}</div>
            </div>`).join('')}
        </div>
      </div>`;
  } catch {
    // No network / not critical — leave the section empty.
  }
}

async function exportHistoryCsv(entries: any[]) {
  const header = 'watched_at,anime_title,episode,anime_id\n';
  const rows = entries.map(e => {
    const iso = new Date(e.watchedAt * 1000).toISOString();
    const title = String(e.animeTitle ?? '').replace(/"/g, '""');
    return `"${iso}","${title}",${e.epNum},${e.animeId}`;
  }).join('\n');
  const csv = header + rows + '\n';

  try {
    const path = await saveDialog({
      defaultPath: 'anigui-watch-history.csv',
      filters: [{ name: 'CSV', extensions: ['csv'] }],
    });
    if (!path) return;
    await invoke('export_history_csv', { path, csv });
    toast('Watch history exported.', 'success');
  } catch (e: any) {
    toast(`Export failed: ${e}`, 'error');
  }
}

// ─── Manage Tab (bulk list operations) ─────────────────────────────────────────

const STATUS_ORDER = ['CURRENT', 'PLANNING', 'PAUSED', 'DROPPED', 'COMPLETED', 'REPEATING'] as const;
const STATUS_LABELS: Record<string, string> = {
  CURRENT: 'Watching', PLANNING: 'Planning', PAUSED: 'Paused',
  DROPPED: 'Dropped', COMPLETED: 'Completed', REPEATING: 'Rewatching',
};

type ManageEntry = { id: number; progress: number; status: string; media: any };
let _manageGroups: Record<string, ManageEntry[]> = {};
let _manageStatus = 'CURRENT';
const _manageSelected = new Set<number>(); // selected media ids

async function renderManage() {
  const content = document.getElementById('profile-content')!;

  if (!state.config.anilist_token) {
    content.innerHTML = `
      <div class="profile-empty fade-in">
        <svg width="52" height="52" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2"><path d="M9 11l3 3L22 4"/><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/></svg>
        <h3>Connect AniList to manage your lists</h3>
        <p>Log in with your AniList account to bulk-move shows between lists or mark whole seasons watched.</p>
        <button class="btn btn-primary" id="manage-open-settings">Open Settings</button>
      </div>`;
    document.getElementById('manage-open-settings')?.addEventListener('click', async () => {
      const { openSettings } = await import('../components/settings');
      openSettings();
    });
    return;
  }

  content.innerHTML = `<div class="profile-loading">
    <div class="skeleton" style="height:40px;border-radius:var(--radius);margin-bottom:16px;"></div>
    <div class="skeleton" style="height:300px;border-radius:var(--radius);"></div>
  </div>`;

  try {
    await fetchManageLists();
    _manageSelected.clear();
    if (!_manageGroups[_manageStatus]?.length) {
      const firstNonEmpty = STATUS_ORDER.find(s => _manageGroups[s]?.length);
      if (firstNonEmpty) _manageStatus = firstNonEmpty;
    }
    drawManage();
  } catch (e: any) {
    content.innerHTML = `<div class="profile-empty fade-in">
      <h3>Failed to load your lists</h3>
      <p style="color:var(--red);font-size:12px;">${esc(String(e))}</p>
    </div>`;
  }
}

async function fetchManageLists() {
  const data = await invoke<any>('get_all_lists');
  const lists = data?.data?.MediaListCollection?.lists ?? [];
  const groups: Record<string, ManageEntry[]> = {};
  for (const l of lists) {
    for (const e of l.entries ?? []) {
      const key = e.status ?? 'CURRENT';
      (groups[key] ??= []).push(e);
    }
  }
  _manageGroups = groups;
}

function drawManage() {
  const content = document.getElementById('profile-content')!;
  const entries = _manageGroups[_manageStatus] ?? [];

  content.innerHTML = `
    <div class="manage-body fade-in">
      <div class="manage-status-pills">
        ${STATUS_ORDER.map(s => `
          <button class="manage-pill${s === _manageStatus ? ' active' : ''}" data-status="${s}">
            ${STATUS_LABELS[s]} <span class="manage-pill-count">${_manageGroups[s]?.length ?? 0}</span>
          </button>`).join('')}
      </div>

      ${entries.length ? `
      <div class="manage-list-header">
        <label class="manage-select-all">
          <input type="checkbox" id="manage-select-all" />
          <span>Select all (${entries.length})</span>
        </label>
      </div>
      <div class="manage-list" id="manage-list">
        ${entries.map(e => {
          const title = e.media.title.english || e.media.title.romaji;
          const eps = e.media.episodes;
          return `
          <label class="manage-row" data-media-id="${e.media.id}">
            <input type="checkbox" class="manage-row-check" data-media-id="${e.media.id}" ${_manageSelected.has(e.media.id) ? 'checked' : ''} />
            <img class="manage-row-cover" src="${e.media.coverImage.medium}" alt="" loading="lazy" onerror="this.style.opacity=0.3"/>
            <div class="manage-row-info">
              <div class="manage-row-title">${esc(title)}</div>
              <div class="manage-row-meta">EP ${e.progress}${eps ? ` / ${eps}` : ''}${!eps ? ' · airing' : ''}</div>
            </div>
          </label>`;
        }).join('')}
      </div>` : `<div class="profile-empty fade-in" style="padding:40px;"><p>Nothing in ${STATUS_LABELS[_manageStatus]}.</p></div>`}

      <div class="manage-action-bar" id="manage-action-bar">
        <span id="manage-selected-count">0 selected</span>
        <div class="manage-action-bar-buttons">
          <div class="dropdown" id="manage-move-dropdown">
            <button class="btn btn-outline dropdown-trigger" id="manage-move-trigger" type="button">Move to… ▾</button>
            <div class="dropdown-menu" id="manage-move-menu">
              ${STATUS_ORDER.filter(s => s !== _manageStatus).map(s => `<button class="dropdown-item" data-value="${s}">${STATUS_LABELS[s]}</button>`).join('')}
            </div>
          </div>
          <button class="btn btn-primary" id="manage-mark-watched-btn">Mark watched</button>
          <button class="btn btn-outline" id="manage-clear-btn">Clear</button>
        </div>
      </div>
    </div>`;

  content.querySelectorAll<HTMLElement>('.manage-pill').forEach(btn => {
    btn.addEventListener('click', () => {
      _manageStatus = btn.dataset.status!;
      _manageSelected.clear();
      drawManage();
    });
  });

  content.querySelectorAll<HTMLInputElement>('.manage-row-check').forEach(cb => {
    cb.addEventListener('change', () => {
      const id = Number(cb.dataset.mediaId);
      if (cb.checked) _manageSelected.add(id); else _manageSelected.delete(id);
      updateManageActionBar();
    });
  });

  const selectAll = document.getElementById('manage-select-all') as HTMLInputElement | null;
  selectAll?.addEventListener('change', () => {
    content.querySelectorAll<HTMLInputElement>('.manage-row-check').forEach(cb => {
      cb.checked = selectAll.checked;
      const id = Number(cb.dataset.mediaId);
      if (selectAll.checked) _manageSelected.add(id); else _manageSelected.delete(id);
    });
    updateManageActionBar();
  });

  document.getElementById('manage-clear-btn')?.addEventListener('click', () => {
    _manageSelected.clear();
    drawManage();
  });

  document.getElementById('manage-mark-watched-btn')?.addEventListener('click', () => runBulkMarkWatched());

  wireDropdown('manage-move-dropdown', 'manage-move-trigger', 'manage-move-menu', (status) => runBulkMove(status));

  updateManageActionBar();
}

function updateManageActionBar() {
  const bar = document.getElementById('manage-action-bar');
  const count = document.getElementById('manage-selected-count');
  if (!bar || !count) return;
  bar.classList.toggle('show', _manageSelected.size > 0);
  count.textContent = `${_manageSelected.size} selected`;
}

async function runBulkMove(status: string) {
  const ids = [..._manageSelected];
  if (!ids.length) return;
  const ops = ids.map(mediaId => ({ mediaId, status }));
  await runBulkUpdate(ops, `Moved to ${STATUS_LABELS[status] ?? status}`);
}

async function runBulkMarkWatched() {
  const entries = _manageGroups[_manageStatus] ?? [];
  const selectedEntries = entries.filter(e => _manageSelected.has(e.media.id));
  const withEpisodes = selectedEntries.filter(e => e.media.episodes);
  const skipped = selectedEntries.length - withEpisodes.length;
  if (!withEpisodes.length) {
    toast('None of the selected shows have a known episode count yet.', 'info');
    return;
  }
  const ops = withEpisodes.map(e => ({ mediaId: e.media.id, progress: e.media.episodes, status: 'COMPLETED' }));
  await runBulkUpdate(ops, 'Marked watched', skipped);
}

async function runBulkUpdate(ops: { mediaId: number; progress?: number; status?: string }[], successLabel: string, skipped = 0) {
  const bar = document.getElementById('manage-action-bar');
  const btns = bar?.querySelectorAll('button, select');
  btns?.forEach(b => (b as HTMLButtonElement).disabled = true);

  try {
    const res = await invoke<any>('bulk_update_entries', { ops });
    const results: { mediaId: number; ok: boolean; error?: string }[] = res?.results ?? [];
    const okCount = results.filter(r => r.ok).length;
    const failCount = results.length - okCount;
    const skippedNote = skipped ? ` · ${skipped} skipped (unknown episode count)` : '';
    toast(
      failCount ? `${successLabel}: ${okCount} updated, ${failCount} failed${skippedNote}` : `${successLabel}: ${okCount} shows${skippedNote}`,
      failCount ? 'error' : 'success'
    );
    _manageSelected.clear();
    await fetchManageLists();
    drawManage();
  } catch (e: any) {
    toast(`Bulk update failed: ${e}`, 'error');
    btns?.forEach(b => (b as HTMLButtonElement).disabled = false);
  }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

function groupByDate(entries: any[]): Map<string, any[]> {
  const map = new Map<string, any[]>();
  const now   = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const yest  = today - 86_400_000;

  for (const e of entries) {
    const d  = new Date(e.watchedAt * 1000);
    const day = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
    const label = day === today ? 'Today'
                : day === yest  ? 'Yesterday'
                : d.toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' });
    if (!map.has(label)) map.set(label, []);
    map.get(label)!.push(e);
  }
  return map;
}

function esc(s: string): string {
  return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}
