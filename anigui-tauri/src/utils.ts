// ─── DOM & Season Utilities ───────────────────────────────────────────────────

export const $ = <T extends HTMLElement>(sel: string, parent: ParentNode = document) =>
  parent.querySelector<T>(sel)!;

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", html = "") {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (html) e.innerHTML = html;
  return e;
}

export function getSeason(date = new Date()): { season: string; year: number } {
  const m = date.getMonth() + 1;
  const year = date.getFullYear();
  const season = m <= 3 ? "WINTER" : m <= 6 ? "SPRING" : m <= 9 ? "SUMMER" : "FALL";
  return { season, year };
}

export function getNextSeason(): { season: string; year: number } {
  const { season, year } = getSeason();
  const seq = ["WINTER", "SPRING", "SUMMER", "FALL"];
  const i = seq.indexOf(season);
  return i === 3 ? { season: "WINTER", year: year + 1 } : { season: seq[i + 1], year };
}

export function seasonLabel(s: string) {
  return s.charAt(0) + s.slice(1).toLowerCase();
}

/** Formats a "seconds from now" countdown as e.g. "2d 4h", "5h 12m", "in a few minutes". */
export function formatCountdown(secondsFromNow: number): string {
  if (secondsFromNow <= 0) return "any moment now";
  const days = Math.floor(secondsFromNow / 86_400);
  const hours = Math.floor((secondsFromNow % 86_400) / 3_600);
  const minutes = Math.floor((secondsFromNow % 3_600) / 60);

  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m`;
  return "in a few minutes";
}
