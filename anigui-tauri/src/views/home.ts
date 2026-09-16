// ─── Home Screen ──────────────────────────────────────────────────────────────
import './home.css';

import { invoke } from '@tauri-apps/api/core';
import { state } from '../state';
import type { Media } from '../types';
import { el, setActiveNav } from '../utils';
import { toast } from '../components/toast';

function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function collectEntries(lists: any[]): Media[] {
  return lists.flatMap((l: any) =>
    l.entries.map((e: any) => ({ ...e.media, mediaListEntry: { id: e.id, progress: e.progress, status: e.status } }))
  );
}

// ─── Card Renderers ───────────────────────────────────────────────────────────

function drawContinueCard(media: Media): HTMLElement {
  const title = media.title.english || media.title.romaji;
  const progress = media.mediaListEntry?.progress ?? 0;
  const eps = media.episodes ?? 0;
  const pct = eps ? Math.round((progress / eps) * 100) : 0;
  const left = eps ? `${pct}% · ${Math.max(eps - progress, 0)} left` : (progress ? `EP ${progress}` : 'Not started');
  const card = el('div', 'home-card');
  card.innerHTML = `
    <div class="home-card-art">
      <img src="${media.coverImage.large || media.coverImage.medium}" alt="" loading="lazy" onerror="this.style.opacity=0.3"/>
      <div class="home-card-bar"><div class="home-card-bar-fill" style="width:${pct}%"></div></div>
      <span class="home-card-ep">${progress ? `EP ${progress}` : 'New'}</span>
    </div>
    <div class="home-card-title">${esc(title)}</div>
    <div class="home-card-meta">${left}</div>
  `;
  card.addEventListener('click', async () => {
    const { selectMedia } = await import('./detail');
    selectMedia(media);
  });
  return card;
}

function drawPosterCard(media: Media): HTMLElement {
  const card = el('article', 'browse-card');
  const title = media.title.english || media.title.romaji;
  const score = media.averageScore;
  const format = media.format?.replace('_', ' ') ?? 'Anime';
  const episodes = media.episodes ? `${media.episodes} eps` : 'Coming soon';
  card.innerHTML = `
    <img src="${media.coverImage.large || media.coverImage.medium}" alt="${esc(title)}" loading="lazy" onerror="this.style.opacity=0.3" />
    <div class="browse-card-overlay">
      <div class="browse-card-topline">${score ? `<span class="browse-card-score">★ ${score}%</span>` : ''}<span>${format}</span></div>
      <span class="browse-card-title">${esc(title)}</span>
      <span class="browse-card-year">${episodes}${media.seasonYear ? ` · ${media.seasonYear}` : ''}</span>
    </div>
  `;
  card.addEventListener('click', async () => {
    const { selectMedia } = await import('./detail');
    selectMedia(media);
  });
  return card;
}

// ─── Home Screen ──────────────────────────────────────────────────────────────

export async function loadHome() {
  setActiveNav('btn-home');
  state.selectedMedia = null;
  const main = document.getElementById('main-panel')!;
  main.innerHTML = `<div class="browse-loading"><div class="spinner"></div><span>Loading home…</span></div>`;

  if (!state.config.anilist_token) {
    try {
      const data = await invoke<any>('get_trending', { page: 1 });
      const items: Media[] = data?.data?.Page?.media ?? [];
      renderLoggedOut(items);
    } catch (e: any) {
      main.innerHTML = `<div class="welcome"><p style="color:var(--red)">Failed to load: ${e}</p></div>`;
    }
    return;
  }

  try {
    const [cwRes, planRes] = await Promise.allSettled([
      invoke<any>('get_continue_watching'),
      invoke<any>('get_planning'),
    ]);
    const continueItems = cwRes.status === 'fulfilled'
      ? collectEntries(cwRes.value?.data?.MediaListCollection?.lists ?? [])
      : [];
    const planningItems = planRes.status === 'fulfilled'
      ? collectEntries(planRes.value?.data?.MediaListCollection?.lists ?? [])
      : [];
    renderLoggedIn(continueItems, planningItems);
  } catch (e: any) {
    main.innerHTML = `<div class="welcome"><p style="color:var(--red)">Failed to load: ${e}</p></div>`;
  }
}

function renderLoggedIn(continueItems: Media[], planningItems: Media[]) {
  const main = document.getElementById('main-panel')!;
  state.sidebarItems = [...continueItems, ...planningItems];

  if (!continueItems.length) {
    main.innerHTML = `
      <div class="home-empty-hero fade-in">
        <span class="home-kicker">WELCOME BACK</span>
        <h1>Nothing in progress right now.</h1>
        <p>Browse something new, or check your plan-to-watch list below.</p>
        <div class="home-hero-actions"><button class="btn btn-primary" id="home-browse-cta">◳ Browse anime</button></div>
      </div>
      <div class="home-rows" id="home-rows"></div>
    `;
  } else {
    const hero = continueItems[0];
    const title = hero.title.english || hero.title.romaji;
    const progress = hero.mediaListEntry?.progress ?? 0;
    const eps = hero.episodes ?? 0;
    const nextEp = progress + 1;
    const pct = eps ? Math.round((progress / eps) * 100) : 0;

    main.innerHTML = `
      <section class="home-hero fade-in" style="background-image:url('${hero.bannerImage || hero.coverImage.large}')">
        <div class="home-hero-scrim"></div>
        <div class="home-hero-body">
          <span class="home-kicker">Continue watching · episode ${progress} of ${eps || '?'}</span>
          <h1>${esc(title)}</h1>
          <p>${eps ? `${pct}% through — pick up right where you left off.` : `Episode ${progress} watched so far.`}</p>
          <div class="home-hero-actions">
            <button class="btn btn-primary" id="home-resume-btn">▶ Resume EP ${nextEp}</button>
            <button class="btn btn-outline" id="home-all-eps-btn">All episodes</button>
          </div>
          ${eps ? `<div class="home-hero-progress"><div class="home-hero-progress-fill" style="width:${pct}%"></div></div>` : ''}
        </div>
      </section>
      <div class="home-rows" id="home-rows"></div>
    `;

    document.getElementById('home-resume-btn')?.addEventListener('click', async (e) => {
      state.selectedMedia = hero;
      state.selectedEp = null;
      const { playEpisode } = await import('./detail');
      playEpisode(nextEp, e.currentTarget as HTMLElement);
    });
    document.getElementById('home-all-eps-btn')?.addEventListener('click', async () => {
      const { selectMedia } = await import('./detail');
      selectMedia(hero);
    });
  }

  document.getElementById('home-browse-cta')?.addEventListener('click', () => document.getElementById('btn-browse')?.click());

  const rows = document.getElementById('home-rows')!;

  if (continueItems.length) {
    const section = el('section', 'home-section fade-in');
    section.innerHTML = `<div class="home-section-header"><span class="home-section-title">Back in rotation</span><span class="browse-count">${continueItems.length} shows watching</span></div><div class="home-scroll"></div>`;
    const scroll = section.querySelector<HTMLElement>('.home-scroll')!;
    continueItems.forEach(m => scroll.appendChild(drawContinueCard(m)));
    rows.appendChild(section);
  }

  if (planningItems.length) {
    const section = el('section', 'home-section fade-in');
    section.innerHTML = `<div class="home-section-header"><span class="home-section-title">Plan to watch</span><span class="browse-count">${planningItems.length} titles</span></div><div class="browse-scroll"></div>`;
    const scroll = section.querySelector<HTMLElement>('.browse-scroll')!;
    planningItems.forEach(m => scroll.appendChild(drawPosterCard(m)));
    rows.appendChild(section);
  }

  if (!continueItems.length && !planningItems.length) {
    rows.innerHTML = `<div class="browse-empty">Nothing on your lists yet. Head to Browse to find something to watch.</div>`;
  }
}

function renderLoggedOut(trendingItems: Media[]) {
  const main = document.getElementById('main-panel')!;
  state.sidebarItems = trendingItems;
  main.innerHTML = `
    <section class="home-empty-hero fade-in">
      <span class="home-kicker">WELCOME TO ANIGUI</span>
      <h1>Track, stream and download anime — all from one desktop app.</h1>
      <p>Log in with AniList to sync progress across devices, or just start browsing.</p>
      <div class="home-hero-actions">
        <button class="btn btn-primary" id="home-login-cta">Log in with AniList</button>
        <button class="btn btn-outline" id="home-browse-cta">◳ Browse anime</button>
      </div>
    </section>
    <div class="home-rows" id="home-rows"></div>
  `;
  document.getElementById('home-login-cta')?.addEventListener('click', async () => {
    const { openSettings } = await import('../components/settings');
    openSettings();
  });
  document.getElementById('home-browse-cta')?.addEventListener('click', () => document.getElementById('btn-browse')?.click());

  const rows = document.getElementById('home-rows')!;
  if (trendingItems.length) {
    const section = el('section', 'home-section fade-in');
    section.innerHTML = `<div class="home-section-header"><span class="home-section-title">Trending now</span><span class="browse-count">${trendingItems.length} titles</span></div><div class="browse-scroll"></div>`;
    const scroll = section.querySelector<HTMLElement>('.browse-scroll')!;
    trendingItems.forEach(m => scroll.appendChild(drawPosterCard(m)));
    rows.appendChild(section);
  }
}

// ─── Search Results ───────────────────────────────────────────────────────────

export async function loadSearchResults(query: string) {
  setActiveNav(null);
  const main = document.getElementById('main-panel')!;
  main.innerHTML = `<div class="browse-loading"><div class="spinner"></div><span>Searching…</span></div>`;
  try {
    const data = await invoke<any>('search_anime', { query, page: 1 });
    const items: Media[] = data?.data?.Page?.media ?? [];
    if (!items.length) {
      main.innerHTML = `<div class="browse-empty">No results for "${esc(query)}".</div>`;
      return;
    }
    state.sidebarItems = items;
    main.innerHTML = `
      <section class="home-section fade-in" style="padding:28px 46px 0">
        <div class="home-section-header"><span class="home-section-title">Results for "${esc(query)}"</span><span class="browse-count">${items.length} titles</span></div>
      </section>
      <div class="browse-grid" id="search-grid" style="padding:14px 46px 40px"></div>
    `;
    const grid = document.getElementById('search-grid')!;
    items.forEach(m => grid.appendChild(drawPosterCard(m)));
  } catch (e: any) {
    toast(`Search failed: ${e}`, 'error');
    main.innerHTML = `<div class="browse-empty">Search failed: ${e}</div>`;
  }
}

// ─── "Watching" Rail Shortcut ─────────────────────────────────────────────────
// Jumps straight to the detail page of whatever's on top of the continue-watching
// list — a shortcut to existing detail/resume functionality, not a new screen.

export async function jumpToWatching() {
  if (!state.config.anilist_token) {
    toast("Log in with AniList to track what you're watching.", 'info');
    return loadHome();
  }
  setActiveNav('btn-watching');
  const main = document.getElementById('main-panel')!;
  main.innerHTML = `<div class="browse-loading"><div class="spinner"></div><span>Loading…</span></div>`;
  try {
    const data = await invoke<any>('get_continue_watching');
    const items = collectEntries(data?.data?.MediaListCollection?.lists ?? []);
    if (!items.length) {
      toast('Nothing in progress — showing Home instead.', 'info');
      return loadHome();
    }
    const { selectMedia } = await import('./detail');
    selectMedia(items[0]);
  } catch (e: any) {
    toast(`Failed to load: ${e}`, 'error');
  }
}
