// ─── Profile Panel — Stats + History ─────────────────────────────────────────
import './profile.css';

import { invoke } from '@tauri-apps/api/core';
import { state } from '../state';
import { toast } from '../components/toast';

type ProfileTab = 'stats' | 'history';
let _activeTab: ProfileTab = 'stats';

// ─── Entry Point ──────────────────────────────────────────────────────────────

export async function loadProfile(tab?: ProfileTab) {
  _activeTab = tab ?? (state.config.anilist_token ? 'stats' : 'history');

  // Deactivate header nav buttons
  document.getElementById('btn-browse')?.classList.remove('active');
  document.getElementById('btn-downloads')?.classList.remove('active');
  document.getElementById('btn-calendar')?.classList.remove('active');

  const main = document.getElementById('main-panel')!;
  main.innerHTML = renderShell();

  // Wire tab switcher
  document.getElementById('profile-tab-stats')!.addEventListener('click', () => switchTab('stats'));
  document.getElementById('profile-tab-history')!.addEventListener('click', () => switchTab('history'));

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
    let html = `<div class="history-body fade-in">
      <div class="history-top-bar">
        <span class="history-count">${entries.length} episode${entries.length !== 1 ? 's' : ''} watched</span>
        <button class="btn btn-outline" id="clear-history-btn" style="font-size:11px;padding:5px 12px;">Clear All</button>
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

  } catch (e: any) {
    content.innerHTML = `<div class="profile-empty fade-in">
      <h3>Failed to load history</h3>
      <p style="color:var(--red);font-size:12px;">${esc(String(e))}</p>
    </div>`;
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
