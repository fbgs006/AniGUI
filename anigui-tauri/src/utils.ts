// ─── DOM & Season Utilities ───────────────────────────────────────────────────

export const $ = <T extends HTMLElement>(sel: string, parent: ParentNode = document) =>
  parent.querySelector<T>(sel)!;

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", html = "") {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (html) e.innerHTML = html;
  return e;
}

/** Clears the active state on every rail nav button and highlights `id` (or none). */
export function setActiveNav(id: string | null) {
  document.querySelectorAll(".rail-btn").forEach(b => b.classList.remove("active"));
  if (id) document.getElementById(id)?.classList.add("active");
}

/**
 * Wires a custom `.dropdown` (trigger button + `.dropdown-item` menu) in place
 * of a native `<select>` — WebView2 renders native select popups using the OS
 * theme rather than the page's `color-scheme`, so a real select clashes with
 * this app's always-dark UI. `onPick` fires with the chosen item's `data-value`.
 */
export function wireDropdown(rootId: string, triggerId: string, menuId: string, onPick: (value: string) => void) {
  const root = document.getElementById(rootId);
  const trigger = document.getElementById(triggerId);
  const menu = document.getElementById(menuId);
  if (!root || !trigger || !menu) return;

  const close = () => {
    menu.classList.remove("open");
    document.removeEventListener("click", onOutsideClick);
    document.removeEventListener("keydown", onKeydown);
  };
  const onOutsideClick = (e: MouseEvent) => {
    if (!root.contains(e.target as Node)) close();
  };
  const onKeydown = (e: KeyboardEvent) => {
    if (e.key === "Escape") close();
  };

  trigger.addEventListener("click", (e) => {
    e.stopPropagation();
    const opening = !menu.classList.contains("open");
    close();
    if (opening) {
      menu.classList.add("open");
      document.addEventListener("click", onOutsideClick);
      document.addEventListener("keydown", onKeydown);
    }
  });

  menu.querySelectorAll<HTMLElement>(".dropdown-item").forEach(item => {
    item.addEventListener("click", () => {
      close();
      onPick(item.dataset.value ?? "");
    });
  });
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
