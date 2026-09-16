// ─── Browse View ──────────────────────────────────────────────────────────────
import './browse.css';

import { invoke } from '@tauri-apps/api/core';
import { state } from '../state';
import type { Media } from '../types';
import { el, setActiveNav } from '../utils';
import { toast } from '../components/toast';
import { getSeason, getNextSeason, seasonLabel } from '../utils';

// ─── Browse Cache (avoid hammering AniList on every click) ────────────────────
let browseCache: { rows: { title: string; items: Media[]; tab?: typeof state.currentTab }[]; ts: number } | null = null;
const BROWSE_CACHE_TTL = 120_000; // 2 minutes

export async function loadBrowse() {
  state.selectedMedia = null;
  setActiveNav("btn-browse");

  const main = document.getElementById("main-panel")!;

  // Use cache if fresh
  if (browseCache && Date.now() - browseCache.ts < BROWSE_CACHE_TTL) {
    renderBrowse(browseCache.rows);
    return;
  }

  main.innerHTML = `<div class="browse-loading"><div class="spinner"></div><span>Loading browse…</span></div>`;

  const { season, year } = getSeason();
  const { season: nextSeason, year: nextYear } = getNextSeason();

  try {
    const results = await Promise.allSettled([
      invoke<any>("get_trending", { page: 1 }),
      invoke<any>("get_popular_this_season", { season, year, page: 1 }),
      invoke<any>("get_upcoming_season", { season: nextSeason, year: nextYear, page: 1 }),
      invoke<any>("get_all_time_popular", { page: 1 }),
    ]);

    const extract = (r: PromiseSettledResult<any>, label: string) => {
      if (r.status === "fulfilled") return r.value;
      console.warn(`[Browse] ${label} failed:`, r.reason);
      return null;
    };

    const [trending, popular, upcoming, allTime] = [
      extract(results[0], "Trending"),
      extract(results[1], "Popular"),
      extract(results[2], "Upcoming"),
      extract(results[3], "All Time"),
    ];

    const rows: { title: string; items: Media[]; tab?: typeof state.currentTab }[] = [
      { title: "🔥 Trending Now", items: trending?.data?.Page?.media ?? [], tab: "trending" },
      { title: `⭐ Popular This Season — ${seasonLabel(season)} ${year}`, items: popular?.data?.Page?.media ?? [] },
      { title: `🗓 Upcoming — ${seasonLabel(nextSeason)} ${nextYear}`, items: upcoming?.data?.Page?.media ?? [] },
      { title: "🏆 All Time Popular", items: allTime?.data?.Page?.media ?? [] },
    ];
    const failed = [trending, popular, upcoming, allTime].map(v => v === null);

    // Merge with previous cache: keep old data only for sections that actually
    // failed this time — a section that genuinely came back empty stays empty.
    if (browseCache) {
      for (let i = 0; i < rows.length; i++) {
        if (failed[i] && browseCache.rows[i]?.items.length > 0) {
          rows[i] = browseCache.rows[i];
        }
      }
    }

    browseCache = { rows, ts: Date.now() };

    renderBrowse(rows);
  } catch (e: any) {
    // Fall back to cache if available
    if (browseCache) {
      renderBrowse(browseCache.rows);
    } else {
      main.innerHTML = `<div class="welcome"><p style="color:var(--red)">Failed to load browse: ${e}</p></div>`;
    }
  }
}

function renderBrowse(rows: { title: string; items: Media[]; tab?: typeof state.currentTab }[]) {
  const main = document.getElementById("main-panel")!;
  const allItems = rows.flatMap(r => r.items);
  const uniqueItemsMap = new Map<number, Media>();
  allItems.forEach(item => uniqueItemsMap.set(item.id, item));
  const uniqueItems = Array.from(uniqueItemsMap.values());
  state.sidebarItems = uniqueItems;

  const genres = [
    "All",
    "Action", "Adventure", "Comedy", "Drama", "Ecchi",
    "Fantasy", "Horror", "Mahou Shoujo", "Mecha", "Music",
    "Mystery", "Psychological", "Romance", "Sci-Fi",
    "Slice of Life", "Sports", "Supernatural", "Thriller",
  ];

  const activeGenres = new Set<string>();

  main.innerHTML = `
    <section class="browse-hero fade-in">
      <div>
        <span class="browse-kicker">DISCOVER ANIME</span>
        <h1>Find your next <em>obsession.</em></h1>
        <p>Fresh seasonal picks, upcoming shows, and the classics everyone keeps talking about.</p>
      </div>
      <div class="browse-stats"><strong>${uniqueItems.length}</strong><span>hand-picked shows<br>to explore</span></div>
    </section>
    <div class="browse-filters fade-in">
      <span class="filter-label">Browse by genre</span>
      ${genres.map((g, i) => `<button class="genre-filter${i === 0 ? " active" : ""}" data-genre="${g}">${g}</button>`).join("")}
      <span class="genre-multi-hint" id="genre-multi-hint" style="display:none;"></span>
    </div>
    <div class="browse-content" data-view="${localStorage.getItem("browseView") || "grid"}"></div>`;

  const contentDiv = main.querySelector<HTMLElement>(".browse-content")!;
  const years = [...new Set(uniqueItems.map(m => m.seasonYear).filter((y): y is number => Boolean(y)))].sort((a, b) => b - a);
  const formats = [...new Set(uniqueItems.map(m => m.format).filter((f): f is string => Boolean(f)))].sort();

  const advanced = el("div", "browse-advanced-filters fade-in");
  advanced.innerHTML = `
    <input id="browse-title-filter" type="search" placeholder="Search titles or genres…" />
    <select id="browse-year-filter"><option value="">Any year</option>${years.map(y => `<option value="${y}">${y}</option>`).join("")}</select>
    <select id="browse-season-filter"><option value="">Any season</option><option value="WINTER">Winter</option><option value="SPRING">Spring</option><option value="SUMMER">Summer</option><option value="FALL">Fall</option></select>
    <select id="browse-format-filter"><option value="">Any format</option>${formats.map(f => `<option value="${f}">${f.replace("_", " ")}</option>`).join("")}</select>
    <select id="browse-sort-filter"><option value="default">Default Sort</option><option value="score_desc">Highest Rated</option><option value="score_asc">Lowest Rated</option><option value="year_desc">Newest</option><option value="year_asc">Oldest</option></select>
    <button class="btn btn-outline" id="browse-clear-filters">Clear</button>
    <button class="btn-icon browse-view-toggle-btn" id="browse-view-toggle" title="Toggle List View">
      ${localStorage.getItem("browseView") === "list"
        ? `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/></svg>`
        : `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/></svg>`
      }
    </button>`;
  main.querySelector(".browse-filters")!.after(advanced);

  const toggleBtn = advanced.querySelector("#browse-view-toggle")!;
  toggleBtn.addEventListener("click", () => {
    let currentView = contentDiv.getAttribute("data-view") || "grid";
    currentView = currentView === "grid" ? "list" : "grid";
    localStorage.setItem("browseView", currentView);
    contentDiv.setAttribute("data-view", currentView);
    const ICON_GRID = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/></svg>`;
    const ICON_LIST = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/></svg>`;
    toggleBtn.innerHTML = currentView === "grid" ? ICON_LIST : ICON_GRID;
  });

  const titleFilter  = advanced.querySelector<HTMLInputElement>("#browse-title-filter")!;
  const yearFilter   = advanced.querySelector<HTMLSelectElement>("#browse-year-filter")!;
  const seasonFilter = advanced.querySelector<HTMLSelectElement>("#browse-season-filter")!;
  const formatFilter = advanced.querySelector<HTMLSelectElement>("#browse-format-filter")!;
  const sortFilter   = advanced.querySelector<HTMLSelectElement>("#browse-sort-filter")!;

  const drawCard = (media: Media, rowItems?: Media[]) => {
    const card = el("article", "browse-card");
    const title = media.title.english || media.title.romaji;
    const score = media.averageScore;
    const format = media.format?.replace("_", " ") ?? "Anime";
    const episodes = media.episodes ? `${media.episodes} eps` : "Coming soon";
    card.innerHTML = `
      <img src="${media.coverImage.large || media.coverImage.medium}" alt="${title}" loading="lazy" onerror="this.style.opacity=0.3" />
      <div class="browse-card-overlay">
        <div class="browse-card-topline">${score ? `<span class="browse-card-score">★ ${score}%</span>` : ""}<span>${format}</span></div>
        <span class="browse-card-title">${title}</span>
        <span class="browse-card-year">${episodes}${media.seasonYear ? ` · ${media.seasonYear}` : ""}</span>
      </div>
      <div class="browse-card-list-info">
        <span class="browse-card-title">${title}</span>
        <div class="browse-card-topline">${score ? `<span class="browse-card-score">★ ${score}%</span>` : ""}<span>${format}</span><span>${episodes}${media.seasonYear ? ` · ${media.seasonYear}` : ""}</span></div>
      </div>
    `;
    card.addEventListener("click", async () => {
      state.sidebarItems = rowItems || uniqueItems;
      const { selectMedia } = await import('./detail');
      selectMedia(media);
    });
    return card;
  };

  let searchPage = 1;
  let hasNextSearchPage = false;
  let searchMatches: Media[] = [];
  let searchLoading = false;
  let searchDebounce: number | null = null;

  const performSearch = async (loadMore = false) => {
    if (!loadMore) {
      searchPage = 1;
      searchMatches = [];
      contentDiv.innerHTML = `<div class="browse-empty">Searching AniList...</div>`;
    } else {
      searchPage++;
      const btn = contentDiv.querySelector("#load-more-btn");
      if (btn) btn.innerHTML = "Loading...";
    }
    searchLoading = true;
    try {
      const query = titleFilter.value.trim();
      const genreList = Array.from(activeGenres);
      const year = yearFilter.value ? Number(yearFilter.value) : null;
      const season = seasonFilter.value || null;
      const format = formatFilter.value || null;
      let sort = null;
      if (sortFilter.value === "score_desc") sort = ["SCORE_DESC"];
      else if (sortFilter.value === "score_asc") sort = ["SCORE"];
      else if (sortFilter.value === "year_desc") sort = ["START_DATE_DESC"];
      else if (sortFilter.value === "year_asc") sort = ["START_DATE"];

      const res = await invoke<any>("advanced_search", { search: query || null, genres: genreList.length ? genreList : null, year, season, format, sort, page: searchPage });
      const pageInfo = res?.data?.Page?.pageInfo;
      hasNextSearchPage = pageInfo?.hasNextPage ?? false;
      const media = res?.data?.Page?.media ?? [];
      if (!loadMore) searchMatches = media;
      else searchMatches.push(...media);

      if (!searchMatches.length) {
        contentDiv.innerHTML = `<div class="browse-empty">Nothing matches your current filters. Try relaxing them.</div>`;
        return;
      }
      contentDiv.innerHTML = "";
      const grid = el("div", "browse-grid fade-in");
      searchMatches.forEach((m: Media) => grid.appendChild(drawCard(m, searchMatches)));
      contentDiv.appendChild(grid);

      if (hasNextSearchPage) {
        const loadMoreBtn = el("button", "btn btn-outline");
        loadMoreBtn.id = "load-more-btn";
        loadMoreBtn.style.cssText = "margin:20px auto;display:block;";
        loadMoreBtn.textContent = "Load More Results";
        loadMoreBtn.addEventListener("click", () => { if (!searchLoading) performSearch(true); });
        contentDiv.appendChild(loadMoreBtn);
      }
    } catch (e) {
      toast(`Search failed: ${e}`, "error");
    } finally {
      searchLoading = false;
    }
  };

  const drawRows = () => {
    const hasFilters = activeGenres.size > 0 || titleFilter.value.trim() || yearFilter.value || seasonFilter.value || formatFilter.value || sortFilter.value !== "default";
    if (hasFilters) {
      if (searchDebounce) clearTimeout(searchDebounce);
      searchDebounce = window.setTimeout(() => performSearch(false), 400);
    } else {
      if (searchDebounce) clearTimeout(searchDebounce);
      contentDiv.innerHTML = "";
      let rendered = 0;
      for (const row of rows) {
        if (!row.items.length) continue;
        rendered++;
        const section = el("section", "browse-section fade-in");
        section.innerHTML = `<div class="browse-section-header"><span class="browse-section-title">${row.title}</span><span class="browse-count">${row.items.length} titles</span></div><div class="browse-scroll"></div>`;
        const scroll = section.querySelector<HTMLElement>(".browse-scroll")!;
        row.items.forEach(media => scroll.appendChild(drawCard(media, row.items)));
        contentDiv.appendChild(section);
      }
      if (!rendered) contentDiv.innerHTML = `<div class="browse-empty">Nothing to show right now.</div>`;
    }
  };

  drawRows();

  const updateGenreHint = () => {
    const hint = main.querySelector<HTMLElement>("#genre-multi-hint");
    if (!hint) return;
    if (activeGenres.size >= 1) {
      hint.textContent = activeGenres.size === 1 ? "1 genre selected · click more to combine" : `${activeGenres.size} genres selected`;
      hint.style.display = "inline";
    } else {
      hint.style.display = "none";
    }
  };

  main.querySelectorAll(".genre-filter").forEach(btn => btn.addEventListener("click", () => {
    const genre = (btn as HTMLElement).dataset.genre!;
    if (genre === "All") {
      activeGenres.clear();
      main.querySelectorAll(".genre-filter").forEach(b => b.classList.remove("active"));
      btn.classList.add("active");
    } else {
      const allBtn = main.querySelector('.genre-filter[data-genre="All"]')!;
      allBtn.classList.remove("active");
      if (activeGenres.has(genre)) {
        activeGenres.delete(genre);
        btn.classList.remove("active");
        if (activeGenres.size === 0) allBtn.classList.add("active");
      } else {
        activeGenres.add(genre);
        btn.classList.add("active");
      }
    }
    updateGenreHint();
    drawRows();
  }));

  [titleFilter, yearFilter, seasonFilter, formatFilter, sortFilter].forEach(f => f.addEventListener("input", drawRows));

  advanced.querySelector("#browse-clear-filters")!.addEventListener("click", () => {
    titleFilter.value = ""; yearFilter.value = ""; seasonFilter.value = ""; formatFilter.value = ""; sortFilter.value = "default";
    activeGenres.clear();
    main.querySelectorAll(".genre-filter").forEach(b => b.classList.remove("active"));
    main.querySelector('.genre-filter[data-genre="All"]')!.classList.add("active");
    updateGenreHint();
    drawRows();
  });
}
