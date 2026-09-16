// ─── Keyboard Shortcuts ───────────────────────────────────────────────────────
import './shortcuts.css';

import { state } from '../state';

const SHORTCUTS = [
  { key: '/',       desc: 'Focus search' },
  { key: 'F',       desc: 'Focus search' },
  { key: 'H',       desc: 'Open Home' },
  { key: 'B',       desc: 'Open Browse' },
  { key: 'W',       desc: 'Jump to what you\'re watching' },
  { key: 'C',       desc: 'Open Calendar' },
  { key: 'P',       desc: 'Open Profile' },
  { key: 'D',       desc: 'Open Downloads' },
  { key: 'Space',   desc: 'Play next episode' },
  { key: 'Esc',     desc: 'Close overlay / go back' },
  { key: '?',       desc: 'Toggle this help' },
];

let overlayVisible = false;

export function wireShortcuts() {
  document.addEventListener('keydown', handleKey);
}

function handleKey(e: KeyboardEvent) {
  // Never fire when typing in inputs / textareas / selects
  const tag = (e.target as HTMLElement).tagName;
  if (['INPUT', 'TEXTAREA', 'SELECT'].includes(tag)) return;
  // Never fire when a modifier (except Shift) is held
  if (e.ctrlKey || e.altKey || e.metaKey) return;

  const key = e.key;

  switch (key) {
    case '?':
      e.preventDefault();
      toggleOverlay();
      break;

    case 'Escape':
      if (overlayVisible) { closeOverlay(); break; }
      // Close any open modal
      document.querySelectorAll<HTMLElement>('.modal-overlay.open').forEach(m => m.classList.remove('open'));
      // Close Browse / Calendar / Profile → go back to detail or welcome
      if (state.selectedMedia) {
        import('../views/detail').then(({ renderDetail }) => renderDetail());
      }
      break;

    case '/':
    case 'f':
    case 'F':
      e.preventDefault();
      (document.getElementById('search-input') as HTMLInputElement)?.focus();
      break;

    case 'h':
    case 'H':
      if (overlayVisible) break;
      document.getElementById('btn-home')?.click();
      break;

    case 'b':
    case 'B':
      if (overlayVisible) break;
      document.getElementById('btn-browse')?.click();
      break;

    case 'w':
    case 'W':
      if (overlayVisible) break;
      document.getElementById('btn-watching')?.click();
      break;

    case 'c':
    case 'C':
      if (overlayVisible) break;
      document.getElementById('btn-calendar')?.click();
      break;

    case 'p':
    case 'P':
      if (overlayVisible) break;
      document.getElementById('login-status')?.click();
      break;

    case 'd':
    case 'D':
      if (overlayVisible) break;
      document.getElementById('btn-downloads')?.click();
      break;

    case ' ':
      if (overlayVisible) break;
      if (!state.selectedMedia) break;
      e.preventDefault();
      const progress = state.selectedMedia.mediaListEntry?.progress ?? 0;
      const nextEp = progress + 1;
      import('../views/detail').then(({ playEpisode }) => playEpisode(nextEp));
      break;
  }
}

// ─── Overlay ──────────────────────────────────────────────────────────────────

export function toggleOverlay() {
  overlayVisible ? closeOverlay() : openOverlay();
}

function openOverlay() {
  overlayVisible = true;
  const overlay = document.getElementById('shortcuts-overlay')!;
  overlay.classList.add('open');
  overlay.focus();
  document.getElementById('btn-shortcuts')?.classList.add('shortcut-active');
}

function closeOverlay() {
  overlayVisible = false;
  document.getElementById('shortcuts-overlay')!.classList.remove('open');
  document.getElementById('btn-shortcuts')?.classList.remove('shortcut-active');
}

// ─── Inject HTML ──────────────────────────────────────────────────────────────

export function injectShortcutsOverlay() {
  const overlay = document.createElement('div');
  overlay.id = 'shortcuts-overlay';
  overlay.className = 'shortcuts-overlay';
  overlay.tabIndex = -1;
  overlay.setAttribute('role', 'dialog');
  overlay.setAttribute('aria-label', 'Keyboard Shortcuts');

  overlay.innerHTML = `
    <div class="shortcuts-modal">
      <div class="shortcuts-header">
        <span class="shortcuts-title">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <rect x="2" y="6" width="20" height="14" rx="2"/>
            <path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M8 14h8"/>
          </svg>
          Keyboard Shortcuts
        </span>
        <button class="shortcuts-close" id="shortcuts-close" aria-label="Close">✕</button>
      </div>
      <div class="shortcuts-grid">
        ${SHORTCUTS.map(s => `
          <div class="shortcut-row">
            <kbd class="shortcut-key">${s.key === ' ' ? 'Space' : s.key}</kbd>
            <span class="shortcut-desc">${s.desc}</span>
          </div>
        `).join('')}
      </div>
      <div class="shortcuts-footer">Press <kbd class="shortcut-key shortcut-key--sm">?</kbd> again to close</div>
    </div>
  `;

  overlay.addEventListener('click', (e) => {
    if (e.target === overlay) closeOverlay();
  });
  overlay.querySelector('#shortcuts-close')!.addEventListener('click', closeOverlay);

  document.getElementById('app')!.appendChild(overlay);
}
