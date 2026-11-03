// ─── Home Screen ──────────────────────────────────────────────────────────────
import './home.css';

import { invoke } from '@tauri-apps/api/core';
import { state } from '../state';
import type { InProgressEntry, Media } from '../types';
import { el, setActiveNav, esc } from '../utils';
import { toast } from '../components/toast';
import { showLoadError } from './browse';
import { formatClock, getInProgress, relativeTime, removeInProgress } from '../watch-progress';


function collectEntries(lists: any[]): Media[] {
  return lists.flatMap((l: any) =>
    l.entries.map((e: any) => ({ ...e.media, mediaListEntry: { id: e.id, progress: e.progress, status: e.status } }))
  );
}

// ─── Local Continue-watching ──────────────────────────────────────────────────
// Entries from in_progress.json: shows started on this machine, including
// episodes left part-way through, which AniList never hears about.

// The media to play/open for a local entry: the AniList copy when there is one,
// since it carries the viewer's current list progress.
function entryMedia(entry: InProgressEntry, fresh: Media[]): Media {
  return fresh.find(m => m.id === entry.animeId) ?? entry.media;
}

/**
 * Local entries still worth showing next to the AniList lists: one AniList
 * already has at or past `ep` was caught up elsewhere and is stale.
 */
function liveLocalEntries(local: InProgressEntry[], fresh: Media[]): InProgressEntry[] {
  return local.filter(e => {
    const progress = fresh.find(f => f.id === e.animeId)?.mediaListEntry?.progress ?? 0;
    return progress < e.ep;
  });
}

// play_episode resumes at the saved position on its own (see resume_position in lib.rs).
async function resumeEntry(media: Media, ep: number, trigger?: HTMLElement) {
  state.selectedMedia = media;
  state.selectedEp = null;
  const { playEpisode } = await import('./detail');
  playEpisode(ep, trigger);
}

function entryPercent(entry: InProgressEntry): number {
  return entry.duration ? Math.min(100, Math.round((entry.timePos / entry.duration) * 100)) : 0;
}

function resumeLabel(entry: InProgressEntry): string {
  if (!entry.timePos) return 'Up next';
  return entry.duration
    ? `${formatClock(entry.timePos)} / ${formatClock(entry.duration)}`
    : `Stopped at ${formatClock(entry.timePos)}`;
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
      <img src="${esc(media.coverImage.large || media.coverImage.medium)}" alt="" loading="lazy"/>
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

// Clicking the art resumes playback; clicking the title opens the detail page.
function drawResumeCard(entry: InProgressEntry, fresh: Media[]): HTMLElement {
  const media = entryMedia(entry, fresh);
  const title = media.title.english || media.title.romaji;
  const card = el('div', 'home-card');
  card.innerHTML = `
    <div class="home-card-art">
      <img src="${esc(media.bannerImage || media.coverImage.large || media.coverImage.medium)}" alt="" loading="lazy"/>
      <div class="home-card-play">▶</div>
      <button class="home-card-dismiss" title="Remove from this list">✕</button>
      <div class="home-card-bar"><div class="home-card-bar-fill" style="width:${entryPercent(entry)}%"></div></div>
      <span class="home-card-ep">EP ${entry.ep}</span>
    </div>
    <div class="home-card-title">${esc(title)}</div>
    <div class="home-card-meta">${resumeLabel(entry)} · ${relativeTime(entry.updatedAt)}</div>
  `;
  card.querySelector('.home-card-art')!.addEventListener('click', () => resumeEntry(media, entry.ep));
  card.querySelector('.home-card-title')!.addEventListener('click', async () => {
    const { selectMedia } = await import('./detail');
    selectMedia(media);
  });
  card.querySelector('.home-card-dismiss')!.addEventListener('click', async (e) => {
    e.stopPropagation();
    await removeInProgress(entry.animeId);
    refreshHomeIfShown();
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
    <img src="${esc(media.coverImage.large || media.coverImage.medium)}" alt="${esc(title)}" loading="lazy" />
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

// ─── Shared Sections ──────────────────────────────────────────────────────────

/** Hero for the most recently watched local entry. */
function renderResumeHero(entry: InProgressEntry, media: Media) {
  const main = document.getElementById('main-panel')!;
  const title = media.title.english || media.title.romaji;
  const pct = entryPercent(entry);
  const blurb = entry.timePos
    ? `Episode ${entry.ep} — stopped at ${formatClock(entry.timePos)}${entry.duration ? ` of ${formatClock(entry.duration)}` : ''}. Pick up right where you left off.`
    : `Episode ${entry.ep} is up next.`;
  main.innerHTML = `
    <section class="home-hero fade-in" style="background-image:url('${esc(media.bannerImage || media.coverImage.large)}')">
      <div class="home-hero-scrim"></div>
      <div class="home-hero-body">
        <span class="home-kicker">Continue watching · ${relativeTime(entry.updatedAt)}</span>
        <h1>${esc(title)}</h1>
        <p>${blurb}</p>
        <div class="home-hero-actions">
          <button class="btn btn-primary" id="home-resume-btn">▶ ${entry.timePos ? 'Resume' : 'Play'} EP ${entry.ep}</button>
          <button class="btn btn-outline" id="home-all-eps-btn">All episodes</button>
        </div>
        ${pct ? `<div class="home-hero-progress"><div class="home-hero-progress-fill" style="width:${pct}%"></div></div>` : ''}
      </div>
    </section>
    <div class="home-rows" id="home-rows"></div>
  `;
  document.getElementById('home-resume-btn')?.addEventListener('click', (e) =>
    resumeEntry(media, entry.ep, e.currentTarget as HTMLElement));
  document.getElementById('home-all-eps-btn')?.addEventListener('click', async () => {
    const { selectMedia } = await import('./detail');
    selectMedia(media);
  });
}

function appendResumeRow(rows: HTMLElement, entries: InProgressEntry[], fresh: Media[]) {
  if (!entries.length) return;
  const section = el('section', 'home-section fade-in');
  section.innerHTML = `<div class="home-section-header"><span class="home-section-title">Pick up where you left off</span><span class="browse-count">${entries.length} show${entries.length !== 1 ? 's' : ''}</span></div><div class="home-scroll"></div>`;
  const scroll = section.querySelector<HTMLElement>('.home-scroll')!;
  entries.forEach(e => scroll.appendChild(drawResumeCard(e, fresh)));
  rows.appendChild(section);
}

// ─── Home Screen ──────────────────────────────────────────────────────────────

export async function loadHome(quiet = false) {
  setActiveNav('btn-home');
  state.selectedMedia = null;
  const main = document.getElementById('main-panel')!;
  if (!quiet) main.innerHTML = `<div class="browse-loading"><div class="spinner"></div><span>Loading home…</span></div>`;

  const local = await getInProgress();

  if (!state.config.anilist_token) {
    try {
      const data = await invoke<any>('get_trending', { page: 1 });
      const items: Media[] = data?.data?.Page?.media ?? [];
      renderLoggedOut(items, local);
    } catch (e: any) {
      if (local.length) renderLoggedOut([], local);
      else showLoadError(main, `Couldn't reach AniList: ${e}`, () => loadHome());
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
    // Both lists failing is an error (offline, expired login), not an empty list.
    if (cwRes.status === 'rejected' && planRes.status === 'rejected') {
      if (!local.length) return showLoadError(main, `Couldn't load your AniList lists: ${cwRes.reason}`, () => loadHome());
      toast(`Couldn't load your AniList lists: ${cwRes.reason}`, 'error');
    }
    renderLoggedIn(continueItems, planningItems, local);
  } catch (e: any) {
    showLoadError(main, `Failed to load: ${e}`, () => loadHome());
  }
}

/** Redraws Home or Watching in place (no spinner) after the lists changed, if one is on screen. */
export function refreshHomeIfShown() {
  if (state.selectedMedia) return;
  if (document.getElementById('watching-page')) loadWatching(true);
  else if (document.getElementById('home-rows')) loadHome(true);
}

function renderLoggedIn(continueItems: Media[], planningItems: Media[], local: InProgressEntry[]) {
  const main = document.getElementById('main-panel')!;
  const fresh = [...continueItems, ...planningItems];
  const localEntries = liveLocalEntries(local, fresh);
  const localIds = new Set(localEntries.map(e => e.animeId));
  // AniList shows already in the local row aren't repeated under "Back in rotation".
  const rotation = continueItems.filter(m => !localIds.has(m.id));
  state.sidebarItems = [...localEntries.map(e => entryMedia(e, fresh)), ...fresh.filter(m => !localIds.has(m.id))];

  if (localEntries.length) {
    renderResumeHero(localEntries[0], entryMedia(localEntries[0], fresh));
  } else if (!continueItems.length) {
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
      <section class="home-hero fade-in" style="background-image:url('${esc(hero.bannerImage || hero.coverImage.large)}')">
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

    document.getElementById('home-resume-btn')?.addEventListener('click', (e) =>
      resumeEntry(hero, nextEp, e.currentTarget as HTMLElement));
    document.getElementById('home-all-eps-btn')?.addEventListener('click', async () => {
      const { selectMedia } = await import('./detail');
      selectMedia(hero);
    });
  }

  document.getElementById('home-browse-cta')?.addEventListener('click', () => document.getElementById('btn-browse')?.click());

  const rows = document.getElementById('home-rows')!;

  appendResumeRow(rows, localEntries, fresh);

  if (rotation.length) {
    const section = el('section', 'home-section fade-in');
    section.innerHTML = `<div class="home-section-header"><span class="home-section-title">Back in rotation</span><span class="browse-count">${rotation.length} shows watching</span></div><div class="home-scroll"></div>`;
    const scroll = section.querySelector<HTMLElement>('.home-scroll')!;
    rotation.forEach(m => scroll.appendChild(drawContinueCard(m)));
    rows.appendChild(section);
  }

  if (planningItems.length) {
    const section = el('section', 'home-section fade-in');
    section.innerHTML = `<div class="home-section-header"><span class="home-section-title">Plan to watch</span><span class="browse-count">${planningItems.length} titles</span></div><div class="browse-scroll"></div>`;
    const scroll = section.querySelector<HTMLElement>('.browse-scroll')!;
    planningItems.forEach(m => scroll.appendChild(drawPosterCard(m)));
    rows.appendChild(section);
  }

  if (!localEntries.length && !continueItems.length && !planningItems.length) {
    rows.innerHTML = `<div class="browse-empty">Nothing on your lists yet. Head to Browse to find something to watch.</div>`;
  }
}

function renderLoggedOut(trendingItems: Media[], local: InProgressEntry[]) {
  const main = document.getElementById('main-panel')!;
  const localIds = new Set(local.map(e => e.animeId));
  state.sidebarItems = [...local.map(e => e.media), ...trendingItems.filter(m => !localIds.has(m.id))];

  if (local.length) {
    renderResumeHero(local[0], local[0].media);
  } else {
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
  }

  const rows = document.getElementById('home-rows')!;
  appendResumeRow(rows, local, []);
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

// ─── Watching Page ────────────────────────────────────────────────────────────
// Everything in progress: shows left part-way on this machine (they carry the
// exact resume point), then the rest of the AniList "Watching" list.

export async function loadWatching(quiet = false) {
  setActiveNav('btn-watching');
  state.selectedMedia = null;
  const main = document.getElementById('main-panel')!;
  if (!quiet) main.innerHTML = `<div class="browse-loading"><div class="spinner"></div><span>Loading what you're watching…</span></div>`;

  const local = await getInProgress();
  let anilist: Media[] = [];
  let listError: unknown = null;
  if (state.config.anilist_token) {
    try {
      const data = await invoke<any>('get_continue_watching');
      anilist = collectEntries(data?.data?.MediaListCollection?.lists ?? []);
    } catch (e) {
      listError = e;
    }
  }
  if (listError && !local.length) {
    return showLoadError(main, `Couldn't load your AniList list: ${listError}`, () => loadWatching());
  }
  if (listError) toast(`Couldn't load your AniList list: ${listError}`, 'error');

  const localEntries = state.config.anilist_token ? liveLocalEntries(local, anilist) : local;
  const localIds = new Set(localEntries.map(e => e.animeId));
  const rest = anilist.filter(m => !localIds.has(m.id));
  state.sidebarItems = [...localEntries.map(e => entryMedia(e, anilist)), ...rest];
  const total = localEntries.length + rest.length;

  main.innerHTML = `
    <div class="watching-page fade-in" id="watching-page">
      <header class="watching-header">
        <span class="home-kicker">Watching</span>
        <h1>What you're watching</h1>
        <p>${total ? `${total} show${total !== 1 ? 's' : ''} in progress.` : 'Nothing in progress yet.'}</p>
      </header>
      <div class="home-rows" id="watching-rows"></div>
    </div>`;
  const rows = document.getElementById('watching-rows')!;

  if (!total) {
    rows.innerHTML = `<div class="browse-empty">${state.config.anilist_token
      ? 'Start an episode, or mark a show as Watching on its page, and it will show up here.'
      : 'Start an episode and it will show up here. Log in with AniList to see your whole Watching list.'}</div>`;
    return;
  }

  if (localEntries.length) {
    const section = el('section', 'home-section');
    section.innerHTML = `<div class="home-section-header"><span class="home-section-title">Pick up where you left off</span><span class="browse-count">${localEntries.length} show${localEntries.length !== 1 ? 's' : ''}</span></div><div class="watching-grid"></div>`;
    const grid = section.querySelector<HTMLElement>('.watching-grid')!;
    localEntries.forEach(e => grid.appendChild(drawResumeCard(e, anilist)));
    rows.appendChild(section);
  }

  if (rest.length) {
    const section = el('section', 'home-section');
    section.innerHTML = `<div class="home-section-header"><span class="home-section-title">On your AniList list</span><span class="browse-count">${rest.length} show${rest.length !== 1 ? 's' : ''}</span></div><div class="watching-grid"></div>`;
    const grid = section.querySelector<HTMLElement>('.watching-grid')!;
    rest.forEach(m => grid.appendChild(drawContinueCard(m)));
    rows.appendChild(section);
  }
}
