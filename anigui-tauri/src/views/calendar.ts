// ─── Airing Calendar View ─────────────────────────────────────────────────────
import './calendar.css';

import { invoke } from '@tauri-apps/api/core';
import { state } from '../state';
import { toast } from '../components/toast';

// ─── Entry Point ──────────────────────────────────────────────────────────────

export async function loadCalendar() {
  // Toggle active state
  document.getElementById('btn-calendar')?.classList.add('active');
  document.getElementById('btn-browse')?.classList.remove('active');
  document.getElementById('btn-downloads')?.classList.remove('active');

  const main = document.getElementById('main-panel')!;
  main.innerHTML = `
    <div class="calendar-panel fade-in">
      <div class="calendar-header">
        <div class="calendar-header-left">
          <h2 class="calendar-title">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="4" width="18" height="18" rx="2"/>
              <line x1="16" y1="2" x2="16" y2="6"/>
              <line x1="8" y1="2" x2="8" y2="6"/>
              <line x1="3" y1="10" x2="21" y2="10"/>
            </svg>
            Airing This Week
          </h2>
          <span class="calendar-subtitle">7-day schedule from AniList</span>
        </div>
      </div>
      <div class="calendar-grid" id="calendar-grid">
        ${renderSkeletons()}
      </div>
    </div>
  `;

  try {
    const data = await invoke<any>('get_airing_schedule', { daysAhead: 7 });
    const schedules: any[] = data?.data?.Page?.airingSchedules ?? [];
    buildGrid(schedules);
  } catch (e: any) {
    const grid = document.getElementById('calendar-grid');
    if (grid) grid.innerHTML = `<div class="calendar-empty">
      <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
      <span>Failed to load airing schedule</span>
    </div>`;
    toast('Failed to load airing schedule.', 'error');
  }
}

// ─── Grid Builder ─────────────────────────────────────────────────────────────

function buildGrid(schedules: any[]) {
  const grid = document.getElementById('calendar-grid')!;
  const nowMs = Date.now();
  const today = new Date();
  const todayStart = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();

  // Build 7 day buckets
  const days: Array<{ label: string; shortLabel: string; isToday: boolean; entries: any[] }> = [];
  for (let i = 0; i < 7; i++) {
    const d = new Date(todayStart + i * 86_400_000);
    const isToday = i === 0;
    const label      = isToday ? 'Today' : i === 1 ? 'Tomorrow'
                     : d.toLocaleDateString('en', { weekday: 'long' });
    const shortLabel = isToday ? 'Today' : i === 1 ? 'Tmrw'
                     : d.toLocaleDateString('en', { weekday: 'short' });
    days.push({ label, shortLabel, isToday, entries: [] });
  }

  // Bucket each schedule entry into its day
  for (const s of schedules) {
    const d    = new Date(s.airingAt * 1000);
    const day  = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
    const idx  = Math.round((day - todayStart) / 86_400_000);
    if (idx >= 0 && idx < 7) days[idx].entries.push(s);
  }

  grid.innerHTML = days.map(day => `
    <div class="cal-day${day.isToday ? ' cal-day--today' : ''}">
      <div class="cal-day-header">
        <span class="cal-day-name">${day.label}</span>
        ${day.isToday ? '<span class="cal-today-badge">Today</span>' : ''}
      </div>
      <div class="cal-day-entries">
        ${day.entries.length === 0
          ? `<div class="cal-empty-day">No episodes</div>`
          : day.entries.map(s => {
              const media = s.media;
              const title = media.title.english || media.title.romaji;
              const airMs = s.airingAt * 1000;
              const isPast = airMs < nowMs;
              const timeStr = new Date(airMs).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
              const score = media.averageScore ? `★ ${media.averageScore}%` : '';
              return `
                <div class="cal-entry${isPast ? ' cal-entry--past' : ''}"
                     data-id="${media.id}" data-title="${esc(title)}" title="${esc(title)}">
                  <img class="cal-cover" src="${media.coverImage.medium}" alt="" loading="lazy" onerror="this.style.opacity=0.3"/>
                  <div class="cal-entry-body">
                    <div class="cal-entry-title">${esc(title)}</div>
                    <div class="cal-entry-badges">
                      <span class="cal-ep-badge">EP ${s.episode}</span>
                      <span class="cal-time-badge${isPast ? ' cal-time-badge--past' : ''}">${isPast ? '✓ Aired' : timeStr}</span>
                      ${score ? `<span class="cal-score-badge">${score}</span>` : ''}
                    </div>
                  </div>
                </div>`;
            }).join('')}
      </div>
    </div>
  `).join('');

  // Wire click → detail panel
  grid.querySelectorAll<HTMLElement>('.cal-entry').forEach(card => {
    card.addEventListener('click', async () => {
      const title = card.dataset.title ?? '';
      const id    = Number(card.dataset.id);
      if (!title) return;
      toast(`Loading ${title}…`, 'info');
      try {
        const res = await invoke<any>('search_anime', { query: title, page: 1 });
        const results = res?.data?.Page?.media ?? [];
        const match = results.find((r: any) => r.id === id) ?? results[0];
        if (match) {
          document.getElementById('btn-calendar')?.classList.remove('active');
          const { selectMedia } = await import('./detail');
          selectMedia(match);
        }
      } catch (e: any) { toast('Failed to load: ' + e, 'error'); }
    });
  });
}

// ─── Skeleton ─────────────────────────────────────────────────────────────────

function renderSkeletons(): string {
  return Array.from({ length: 7 }, () => `
    <div class="cal-day">
      <div class="cal-day-header">
        <div class="skeleton" style="height:14px;width:60px;border-radius:4px;"></div>
      </div>
      <div class="cal-day-entries">
        ${Array.from({ length: Math.floor(Math.random() * 3) + 1 }, () => `
          <div class="skeleton" style="height:72px;border-radius:10px;margin-bottom:6px;"></div>
        `).join('')}
      </div>
    </div>
  `).join('');
}

function esc(s: string): string {
  return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}
