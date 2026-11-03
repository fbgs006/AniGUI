// ─── Watch Progress ───────────────────────────────────────────────────────────
// "Did the user finish this episode?" plus the local continue-watching list
// (in_progress.json), which works logged-out and also remembers episodes left
// part-way through — AniList only ever hears about finished ones.

import { invoke } from "@tauri-apps/api/core";
import type { InProgressEntry, Media } from "./types";

/**
 * Watched = the user asked for the next episode (`advanced`), or got near the
 * outro. Time spent in the player doesn't matter: opening an episode and
 * jumping to the end counts, opening it and closing it early doesn't.
 */
export async function isEpisodeFinished(
  media: Media | null | undefined, ep: number, percent: number, timePos: number, advanced: boolean,
): Promise<boolean> {
  if (advanced) return true;
  let finished = percent > 0.85;
  if (media?.idMal && timePos > 0) {
    try {
      const res = await fetch(`https://api.aniskip.com/v2/skip-times/${media.idMal}/${ep}?types=ed&episodeLength=0`);
      if (res.ok) {
        const data = await res.json();
        const ed = data.results?.find((r: any) => r.skipType === "ed");
        if (ed?.interval?.startTime) finished = timePos >= (ed.interval.startTime - 10);
      }
    } catch (err) {
      console.error("AniSkip fetch failed", err);
    }
  }
  return finished;
}

export async function getInProgress(): Promise<InProgressEntry[]> {
  try {
    return await invoke<InProgressEntry[]>("get_in_progress");
  } catch {
    return [];
  }
}

export async function removeInProgress(animeId: number) {
  try { await invoke("remove_in_progress", { animeId }); } catch { /* non-critical */ }
}

/**
 * Records where the user stopped. An unfinished episode is kept with its
 * position (the backend resumes it there); a finished one moves the show on to
 * the next episode, or off the list once the last known episode is done.
 */
export async function recordProgress(media: Media, ep: number, timePos: number, duration: number, finished: boolean) {
  if (finished && media.episodes && ep >= media.episodes) {
    return removeInProgress(media.id);
  }
  const { mediaListEntry: _stale, relations: _big, ...snapshot } = media;
  const entry: InProgressEntry = {
    animeId:   media.id,
    media:     snapshot,
    ep:        finished ? ep + 1 : ep,
    timePos:   finished ? 0 : Math.floor(timePos),
    duration:  finished ? 0 : Math.floor(duration),
    updatedAt: Math.floor(Date.now() / 1000),
  };
  try { await invoke("save_in_progress", { entry }); } catch { /* non-critical */ }
}

export function formatClock(sec: number): string {
  const s = Math.max(0, Math.floor(sec));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

export function relativeTime(unixSec: number): string {
  const diff = Date.now() / 1000 - unixSec;
  if (diff < 60) return "Just now";
  if (diff < 3600) return `${Math.floor(diff / 60)} min ago`;
  const then = new Date(unixSec * 1000);
  const today = new Date(); today.setHours(0, 0, 0, 0);
  const days = Math.floor((today.getTime() - new Date(then).setHours(0, 0, 0, 0)) / 86_400_000);
  if (days <= 0) return then.getHours() >= 18 ? "Tonight" : "Today";
  if (days === 1) return then.getHours() >= 18 ? "Last night" : "Yesterday";
  if (days < 7) return `${days} days ago`;
  return then.toLocaleDateString();
}
