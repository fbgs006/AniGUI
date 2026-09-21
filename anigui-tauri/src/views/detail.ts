// ─── Detail Panel + Playback ─────────────────────────────────────────────────
import './detail.css';

import { invoke } from '@tauri-apps/api/core';
import { state } from '../state';
import type { Media } from '../types';
import { $, el, formatCountdown, setActiveNav, wireDropdown } from '../utils';
import { toast } from '../components/toast';
import { enqueueDownload } from '../components/download-queue';

// ─── Relations Helpers ────────────────────────────────────────────────────────

const RELATION_LABEL: Record<string, string> = {
  PREQUEL: "Prequel", SEQUEL: "Sequel", SIDE_STORY: "Side Story",
  PARENT: "Parent Story", ALTERNATIVE: "Alternative", SPIN_OFF: "Spin-off",
  SUMMARY: "Summary", CHARACTER: "Character", OTHER: "Other",
};

const DETAIL_STATUS_LABELS: Record<string, string> = {
  "Not in List": "Not in List",
  CURRENT: "Watching",
  COMPLETED: "Completed",
  PLANNING: "Plan to Watch",
  DROPPED: "Dropped",
  PAUSED: "Paused",
};

function renderRelations(m: Media): string {
  const edges = m.relations?.edges ?? [];
  const relevant = edges.filter(e =>
    ["PREQUEL","SEQUEL","SIDE_STORY","PARENT","ALTERNATIVE","SPIN_OFF"].includes(e.relationType)
  );
  if (!relevant.length) return "";

  const cards = relevant.map(e => {
    const t = e.node.title.english || e.node.title.romaji;
    const label = RELATION_LABEL[e.relationType] ?? e.relationType;
    const fmt = e.node.format ? e.node.format.replace(/_/g, " ") : "";
    return `
      <div class="relation-card" data-id="${e.node.id}" title="${t}">
        <img src="${e.node.coverImage.medium}" alt="" onerror="this.style.opacity=0.3" />
        <div class="relation-info">
          <span class="relation-type">${label}</span>
          <span class="relation-title">${t}</span>
          <span class="relation-fmt">${fmt}</span>
        </div>
      </div>`;
  }).join("");

  return `
    <div class="card fade-in" id="relations-card">
      <div class="section-title">Related</div>
      <div class="relations-grid">${cards}</div>
    </div>`;
}

function attachRelationClicks() {
  document.querySelectorAll(".relation-card").forEach(card => {
    card.addEventListener("click", async () => {
      const id = Number((card as HTMLElement).dataset.id);
      const title = (card as HTMLElement).title;
      if (!id || !title) return;
      toast(`Loading ${title}…`, "info");
      try {
        const data = await invoke<any>("search_anime", { query: title, page: 1 });
        const results: Media[] = data?.data?.Page?.media ?? [];
        const match = results.find(r => r.id === id) ?? results[0];
        if (match) {
          state.sidebarItems = results;
          selectMedia(match);
        }
      } catch (e: any) {
        toast("Failed to load: " + e, "error");
      }
    });
  });
}

// ─── Playback ─────────────────────────────────────────────────────────────────

export async function playEpisode(ep: number, triggerElement?: HTMLElement) {
  if (!state.selectedMedia) return;
  if (state.playLaunching) { toast("A video player is already opening.", "info"); return; }

  state.playLaunching = true;
  state.activePlayingAnimeId = state.selectedMedia.id;
  state.activePlayingEp = ep;

  const chip = document.querySelector<HTMLElement>(`.ep-chip[data-ep="${ep}"]`);
  if (triggerElement) {
    triggerElement.dataset.originalText = triggerElement.dataset.originalText || triggerElement.innerHTML;
    triggerElement.innerHTML = "⏳ Playing...";
    triggerElement.classList.add("playing-active");
  } else {
    const pb = document.getElementById("btn-play-selected");
    const nb = document.getElementById("play-next");
    if (pb) pb.innerHTML = "⏳ Playing...";
    if (nb) nb.innerHTML = "⏳ Playing...";
  }
  if (chip) chip.classList.add("playing-active");

  const title = state.selectedMedia.title.english || state.selectedMedia.title.romaji;
  toast(`Launching EP ${ep}…`, "info");

  const result = await invoke<{ error?: string; success?: boolean }>("play_episode", {
    title,
    epNum: ep,
    animeId: state.selectedMedia.id,
    malId: state.selectedMedia.idMal ?? null,
    totalEps: state.selectedMedia.episodes ?? 0,
  });
  if (result.error) {
    toast(result.error, "error");
    state.playLaunching = false;
    resetPlayButtons();
  }
}

/** The player's next/previous button switched episode without going through the UI. */
export function setPlayingEpisode(ep: number) {
  state.activePlayingEp = ep;
  document.querySelectorAll(".ep-chip.playing-active").forEach(el => el.classList.remove("playing-active"));
  document.querySelector(`.ep-chip[data-ep="${ep}"]`)?.classList.add("playing-active");
  toast(`Now playing EP ${ep}`, "info");
}

/**
 * Records `ep` as the new AniList progress for `animeId` everywhere it's shown,
 * and redraws the detail page if it's on screen (the progress bar, the
 * "Play EP n" button and the watched chips all derive from it).
 */
export function applySyncedProgress(animeId: number, ep: number) {
  const apply = (m: Media) => {
    if (m.mediaListEntry) m.mediaListEntry.progress = ep;
    else m.mediaListEntry = { id: 0, progress: ep, status: "CURRENT" };
  };

  state.sidebarItems.filter(m => m.id === animeId).forEach(apply);
  if (state.selectedMedia?.id !== animeId) return;
  apply(state.selectedMedia);

  // Don't yank the user back from another tab just because a sync landed.
  if (document.querySelector("#main-panel .detail-hero")) renderDetail();
}

export function resetPlayButtons() {
  // Clear the "currently playing" pointer so a finished episode doesn't keep
  // matching activePlayingAnimeId/Ep on the next render — otherwise buildEpGrid()
  // and renderDetail() re-apply .playing-active (pointer-events: none) to that
  // chip/button forever, making it look like the play button "stopped working".
  state.activePlayingAnimeId = null;
  state.activePlayingEp = null;
  document.querySelectorAll(".playing-active").forEach(el => {
    el.classList.remove("playing-active");
    if ((el as HTMLElement).dataset.originalText) {
      el.innerHTML = (el as HTMLElement).dataset.originalText!;
    }
  });
}

export function downloadEpisode(ep: number) {
  if (!state.selectedMedia) return;
  enqueueDownload(state.selectedMedia, ep);
}

// ─── Episode Grid ─────────────────────────────────────────────────────────────

function buildEpGrid(eps: number, progress: number) {
  const grid = document.getElementById("ep-grid");
  if (!grid) return;
  grid.innerHTML = "";

  for (let i = 1; i <= eps; i++) {
    const chip = el("div", `ep-chip${i <= progress ? " watched" : i === progress + 1 ? " next" : ""}`);
    chip.textContent = String(i);
    chip.dataset.ep = String(i);

    chip.addEventListener("click", () => {
      if (state.selectedEp === i) {
        state.selectedEp = null;
        chip.classList.remove("selected");
        document.getElementById("ep-actions")?.classList.add("hidden");
      } else {
        document.querySelectorAll(".ep-chip").forEach(c => c.classList.remove("selected"));
        chip.classList.add("selected");
        state.selectedEp = i;
        const actions = document.getElementById("ep-actions");
        if (actions) {
          actions.classList.remove("hidden");
          const playBtn = document.getElementById("btn-play-selected");
          if (playBtn) {
            if (state.selectedMedia?.id === state.activePlayingAnimeId && i === state.activePlayingEp) {
              playBtn.dataset.originalText = playBtn.dataset.originalText || playBtn.innerHTML;
              playBtn.innerHTML = "⏳ Playing...";
              playBtn.classList.add("playing-active");
            } else if (playBtn.dataset.originalText) {
              playBtn.innerHTML = playBtn.dataset.originalText;
              playBtn.classList.remove("playing-active");
            }
          }
        }
      }
    });

    chip.addEventListener("dblclick", (e) => playEpisode(i, e.currentTarget as HTMLElement));

    if (state.selectedMedia?.id === state.activePlayingAnimeId && i === state.activePlayingEp) {
      chip.classList.add("playing-active");
    }
    grid.appendChild(chip);
  }

  document.getElementById("btn-play-selected")?.addEventListener("click", (e) => {
    if (state.selectedEp) playEpisode(state.selectedEp, e.currentTarget as HTMLElement);
  });
  document.getElementById("btn-dl-selected")?.addEventListener("click", () => {
    if (state.selectedEp) downloadEpisode(state.selectedEp);
  });
}

// ─── Detail Panel ─────────────────────────────────────────────────────────────

export function renderDetail() {
  if (!state.selectedMedia) return;
  const m = state.selectedMedia;
  const title = m.title.english || m.title.romaji;
  const progress = m.mediaListEntry?.progress ?? 0;
  const eps = m.episodes ?? 0;
  const pct = eps ? Math.round((progress / eps) * 100) : 0;
  const nextEp = progress + 1;

  const main = $("#main-panel");
  const banner = m.bannerImage || m.coverImage.large || m.coverImage.medium;
  main.innerHTML = `
    <div class="detail-banner fade-in" style="background-image:url('${banner}')"><div class="detail-banner-scrim"></div></div>
    <div class="detail-hero fade-in">
      <img class="detail-cover" src="${m.coverImage.large || m.coverImage.medium}" alt="${title}" />
      <div class="detail-info">
        <div class="detail-title">${title}</div>
        <div class="detail-meta-row">
          ${m.genres.slice(0, 4).map(g => `<span class="genre-pill">${g}</span>`).join("")}
          ${m.averageScore ? `<span class="score-badge">★ ${m.averageScore}%</span>` : ""}
          ${m.season ? `<span class="season-badge">${m.season} ${m.seasonYear ?? ""}</span>` : ""}
          ${m.status === "RELEASING" && m.nextAiringEpisode
            ? `<span class="airing-badge">EP ${m.nextAiringEpisode.episode} airing in ${formatCountdown(m.nextAiringEpisode.timeUntilAiring)}</span>`
            : ""}
        </div>
        ${m.description ? `
        <div class="detail-desc-wrap">
          <p class="detail-desc" id="detail-desc">${m.description.replace(/<[^>]*>/g, "").trim()}</p>
          <button type="button" class="detail-desc-toggle" id="detail-desc-toggle">Show more</button>
        </div>` : ""}
        ${eps ? `
        <div class="progress-section">
          <div class="progress-label">
            <span>Progress</span>
            <span>EP ${progress}/${eps} · ${pct}%</span>
          </div>
          <div class="progress-bar"><div class="progress-bar-fill" style="width:${pct}%"></div></div>
        </div>` : ""}
        <div class="action-row">
          ${eps && nextEp <= eps ? `<button class="btn btn-primary" id="play-next">▶ Play EP ${nextEp}</button>` : ""}
          <div class="dropdown dropdown-down" id="detail-status-dropdown">
            <button type="button" class="status-select dropdown-trigger" id="detail-status-trigger"></button>
            <div class="dropdown-menu" id="detail-status-menu">
              ${Object.entries(DETAIL_STATUS_LABELS).map(([value, label]) =>
                `<button type="button" class="dropdown-item" data-value="${value}">${label}</button>`
              ).join("")}
            </div>
          </div>
        </div>
      </div>
    </div>

    <div class="detail-body">
      ${eps ? `
      <div class="card fade-in">
        <div class="section-title">Episodes</div>
        <div class="ep-grid" id="ep-grid"></div>
        <div style="margin-top:16px;display:flex;gap:10px;" id="ep-actions" class="hidden">
          <button class="btn btn-primary" id="btn-play-selected">▶ Play</button>
          <button class="btn btn-outline" id="btn-dl-selected">⬇ Download</button>
        </div>
      </div>` : ""}

      ${renderRelations(m)}
    </div>
  `;

  // Description "Show more" toggle — only needed when the text actually overflows
  // the 4-line clamp, since AniList descriptions vary a lot in length.
  const descEl = document.getElementById("detail-desc");
  const descToggle = document.getElementById("detail-desc-toggle") as HTMLButtonElement | null;
  if (descEl && descToggle && descEl.scrollHeight > descEl.clientHeight + 1) {
    descToggle.classList.add("show");
    descToggle.addEventListener("click", () => {
      const expanded = descEl.classList.toggle("expanded");
      descToggle.textContent = expanded ? "Show less" : "Show more";
    });
  }

  // Airing shows with unknown episode count
  if (!eps) {
    const actions = main.querySelector<HTMLElement>(".action-row");
    if (actions) {
      actions.insertAdjacentHTML("afterbegin",
        `<button class="btn btn-primary" id="play-next">Play EP ${nextEp}</button>
         <div class="manual-episode">
           <label for="manual-episode">Episode</label>
           <input id="manual-episode" type="number" min="1" value="${nextEp}" />
           <button class="btn btn-outline" id="play-manual">Play</button>
         </div>`
      );
    }
  }

  // Status dropdown (custom — a native <select>'s popup renders per the OS
  // theme in WebView2, not the app's dark color-scheme, so text can end up
  // unreadable; see the note on wireDropdown() in utils.ts).
  const statusTrigger = document.getElementById("detail-status-trigger") as HTMLButtonElement;
  const currentStatus = m.mediaListEntry?.status ?? "Not in List";
  statusTrigger.textContent = `${DETAIL_STATUS_LABELS[currentStatus] ?? currentStatus} ▾`;
  statusTrigger.disabled = !state.config.anilist_token;
  wireDropdown("detail-status-dropdown", "detail-status-trigger", "detail-status-menu", async (value) => {
    if (!m.id || !value) return;
    statusTrigger.textContent = `${DETAIL_STATUS_LABELS[value] ?? value} ▾`;
    await invoke("update_status", { mediaId: m.id, status: value });
    toast("Status updated!", "success");
  });

  // Play-next button
  const nextBtn = document.getElementById("play-next");
  if (nextBtn) {
    nextBtn.addEventListener("click", (e) => playEpisode(nextEp, e.currentTarget as HTMLElement));
    if (state.activePlayingAnimeId === m.id && state.activePlayingEp === nextEp) {
      nextBtn.dataset.originalText = nextBtn.innerHTML;
      nextBtn.innerHTML = "⏳ Playing...";
      nextBtn.classList.add("playing-active");
    }
  }

  // Manual episode button
  const manualBtn = document.getElementById("play-manual");
  if (manualBtn) {
    manualBtn.addEventListener("click", (e) => {
      const ep = Number((document.getElementById("manual-episode") as HTMLInputElement).value);
      if (Number.isInteger(ep) && ep > 0) playEpisode(ep, e.currentTarget as HTMLElement);
      else toast("Enter a valid episode number.", "error");
    });
  }

  if (eps) buildEpGrid(eps, progress);
  attachRelationClicks();
}

// ─── Select Media ─────────────────────────────────────────────────────────────

export function selectMedia(media: Media) {
  state.selectedMedia = media;
  state.selectedEp = null;
  setActiveNav('btn-watching');
  renderDetail();
}
