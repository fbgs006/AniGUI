// ─── Sidebar View ─────────────────────────────────────────────────────────────
import './sidebar.css';

import { invoke } from '@tauri-apps/api/core';
import { state } from '../state';
import { el } from '../utils';
import { toast } from '../components/toast';

// selectMedia is imported lazily inside event handlers to avoid circular deps
// (detail imports sidebar, sidebar imports detail)

export function renderSidebar() {
  const list = document.getElementById("sidebar-list")!;
  list.innerHTML = "";

  if (state.sidebarItems.length === 0) {
    const msg = state.currentTab === "continue" ? "Log in to see your watch list."
      : state.currentTab === "planning" ? "Log in to see your plan-to-watch list."
      : "Nothing to show.";
    list.innerHTML = `<div class="empty-state">
      <svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="12" cy="12" r="10"/><path d="M8 12h8M12 8v8"/></svg>
      <span>${msg}</span>
    </div>`;
    return;
  }

  state.sidebarItems.forEach((media) => {
    const title    = media.title.english || media.title.romaji;
    const cover    = media.coverImage.medium;
    const progress = media.mediaListEntry?.progress ?? 0;
    const eps      = media.episodes ?? 0;
    const pct      = eps ? Math.round((progress / eps) * 100) : 0;
    const isActive = state.selectedMedia?.id === media.id;

    const item = el("div", `sidebar-item${isActive ? " active" : ""}`);
    item.innerHTML = `
      <img class="item-cover" src="${cover}" alt="" loading="lazy" onerror="this.style.opacity=0.3"/>
      <div class="item-info">
        <div class="item-title">${title}</div>
        <div class="item-meta">${progress ? `EP ${progress}${eps ? "/" + eps : ""}` : (eps ? eps + " eps" : "Ongoing")}</div>
        ${eps ? `<div class="progress-bar-mini"><div class="progress-bar-mini-fill" style="width:${pct}%"></div></div>` : ""}
      </div>
    `;
    item.addEventListener("click", async () => {
      const { selectMedia } = await import('./detail');
      selectMedia(media);
    });
    list.appendChild(item);
  });

  if (state.sidebarHasMore) {
    const indicator = el("div", "load-more-indicator");
    indicator.textContent = "Scroll for more…";
    list.appendChild(indicator);
  }
}

export function renderSkeletons(n = 8) {
  const list = document.getElementById("sidebar-list")!;
  list.innerHTML = "";
  for (let i = 0; i < n; i++) {
    const item = el("div", "sidebar-item");
    item.innerHTML = `
      <div class="item-cover skeleton"></div>
      <div class="item-info">
        <div class="skeleton" style="height:12px;width:90%;border-radius:4px;margin-bottom:6px;"></div>
        <div class="skeleton" style="height:10px;width:60%;border-radius:4px;"></div>
      </div>`;
    list.appendChild(item);
  }
}

export async function loadTab(tab: typeof state.currentTab, query = "", page = 1) {
  if (page === 1) {
    state.currentTab = tab;
    document.getElementById("btn-browse")?.classList.remove("active");
    document.getElementById("btn-downloads")?.classList.remove("active");
    state.sidebarPage  = 1;
    state.sidebarItems = [];
    document.querySelectorAll(".tab-btn").forEach(b => {
      b.classList.toggle("active", (b as HTMLElement).dataset.tab === tab);
    });
    renderSkeletons();
  }

  state.sidebarLoading = true;
  try {
    let items: any[] = [];
    let hasMore = false;

    if (tab === "trending") {
      const data = await invoke<any>("get_trending", { page });
      items   = data?.data?.Page?.media ?? [];
      hasMore = data?.data?.Page?.pageInfo?.hasNextPage ?? false;
    } else if (tab === "continue") {
      if (!state.config.anilist_token) {
        toast("Log in with AniList to see your watch list.", "info");
      } else {
        const data  = await invoke<any>("get_continue_watching");
        const lists = data?.data?.MediaListCollection?.lists ?? [];
        items = lists.flatMap((l: any) =>
          l.entries.map((e: any) => ({ ...e.media, mediaListEntry: { id: e.id, progress: e.progress, status: e.status } }))
        );
      }
    } else if (tab === "planning") {
      if (!state.config.anilist_token) {
        toast("Log in with AniList to see your plan-to-watch list.", "info");
      } else {
        const data  = await invoke<any>("get_planning");
        const lists = data?.data?.MediaListCollection?.lists ?? [];
        items = lists.flatMap((l: any) =>
          l.entries.map((e: any) => ({ ...e.media, mediaListEntry: { id: e.id, progress: e.progress, status: e.status } }))
        );
      }
    } else if (tab === "search" && query) {
      const data = await invoke<any>("search_anime", { query, page });
      items   = data?.data?.Page?.media ?? [];
      hasMore = data?.data?.Page?.pageInfo?.hasNextPage ?? false;
    }

    if (page === 1) {
      state.sidebarItems = items;
    } else {
      state.sidebarItems = [...state.sidebarItems, ...items];
    }
    state.sidebarHasMore = hasMore;
    state.sidebarPage    = page;
  } catch (e: any) {
    toast("Failed to load: " + e, "error");
    if (page === 1) state.sidebarItems = [];
  }

  state.sidebarLoading = false;

  // ─── Init View Toggle ─────────────────────────────────────────────────────────
  const listEl = document.getElementById("sidebar-list")!;
  const toggleBtn = document.getElementById("sidebar-view-toggle")!;
  let currentView = localStorage.getItem("sidebarView") || "card";
  listEl.setAttribute("data-view", currentView);

  const ICON_CARD = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/></svg>`;
  const ICON_LIST = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/></svg>`;

  toggleBtn.innerHTML = currentView === "card" ? ICON_LIST : ICON_CARD;

  toggleBtn.addEventListener("click", () => {
    currentView = currentView === "card" ? "compact" : "card";
    localStorage.setItem("sidebarView", currentView);
    listEl.setAttribute("data-view", currentView);
    toggleBtn.innerHTML = currentView === "card" ? ICON_LIST : ICON_CARD;
  });

  renderSidebar();
}

export async function loadMore() {
  if (state.sidebarLoading || !state.sidebarHasMore) return;
  await loadTab(state.currentTab, "", state.sidebarPage + 1);
}
