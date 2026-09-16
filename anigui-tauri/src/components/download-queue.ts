// ─── Download Queue Manager ──────────────────────────────────────────────────
// Downloads run one at a time (ani-cli spawns a single bash/curl chain per
// call), but the user can queue several episodes ahead of time. This module
// owns state.downloadQueue: it drains the queue sequentially, retries failed
// items a couple of times, and re-renders whatever's currently mounted in the
// Downloads view — matches the app's existing direct-DOM style, no pub/sub.
import './download-queue.css';

import { invoke } from '@tauri-apps/api/core';
import { state } from '../state';
import type { Media, DownloadQueueItem } from '../types';
import { el } from '../utils';
import { toast } from './toast';

const MAX_RETRIES = 2;
let processing = false;

function makeId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function stripAnsi(log: string): string[] {
  const clean = log.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '');
  return clean.split('\n').map(l => l.split('\r').pop() ?? '').filter(l => l.trim() !== '');
}

// ─── Queue Operations ─────────────────────────────────────────────────────────

export function enqueueDownload(media: Media, epNum: number) {
  const title = media.title.english || media.title.romaji;
  const dup = state.downloadQueue.find(
    q => q.animeId === media.id && q.epNum === epNum && (q.status === 'queued' || q.status === 'downloading')
  );
  if (dup) {
    toast(`EP ${epNum} is already queued.`, 'info');
    return;
  }
  state.downloadQueue.push({
    id: makeId(),
    animeId: media.id,
    animeTitle: title,
    cover: media.coverImage.medium,
    epNum,
    status: 'queued',
    attempts: 0,
    log: '',
  });
  toast(`Queued EP ${epNum} — ${title}`, 'info');
  renderQueuePanel();
  processQueue();
}

function activeItem(): DownloadQueueItem | undefined {
  return state.downloadQueue.find(q => q.status === 'downloading');
}

async function processQueue() {
  if (processing) return;
  const next = state.downloadQueue.find(q => q.status === 'queued');
  if (!next) return;

  processing = true;
  next.status = 'downloading';
  next.log = '';
  renderQueuePanel();

  try {
    const result = await invoke<any>('start_download', { title: next.animeTitle, epNum: next.epNum });
    if (result?.error) finishActiveItem(false, result.error);
    // Otherwise resolution happens later via handleDownloadFinished(), driven
    // by the download_finished Tauri event.
  } catch (e: any) {
    finishActiveItem(false, String(e));
  }
}

function finishActiveItem(success: boolean, errorNote?: string) {
  const item = activeItem();
  processing = false;
  if (!item) { processQueue(); return; }

  if (success) {
    item.status = 'done';
    toast(`Downloaded EP ${item.epNum} — ${item.animeTitle}`, 'success');
  } else {
    item.attempts++;
    if (errorNote) item.log += `\n${errorNote}`;
    if (item.attempts <= MAX_RETRIES) {
      item.status = 'queued';
      toast(`EP ${item.epNum} failed — retrying (${item.attempts}/${MAX_RETRIES})…`, 'info');
    } else {
      item.status = 'failed';
      toast(`EP ${item.epNum} — ${item.animeTitle} failed after ${item.attempts} attempts.`, 'error');
    }
  }
  renderQueuePanel();

  // Small delay so the previous child process has fully exited before the next spawn.
  setTimeout(processQueue, 600);
}

// ─── Tauri Event Bridges (called from main.ts) ────────────────────────────────

export function handleDownloadChunk(chunk: string) {
  const item = activeItem();
  if (!item) return;
  item.log += chunk;
  renderQueuePanel();
}

export function handleDownloadFinished(success: boolean) {
  finishActiveItem(success);
}

// ─── Item Actions ─────────────────────────────────────────────────────────────

export function retryItem(id: string) {
  const item = state.downloadQueue.find(q => q.id === id);
  if (!item || item.status === 'downloading') return;
  item.status = 'queued';
  item.attempts = 0;
  item.log = '';
  renderQueuePanel();
  processQueue();
}

export function removeItem(id: string) {
  const item = state.downloadQueue.find(q => q.id === id);
  if (item?.status === 'downloading') {
    toast("Can't remove a download in progress.", 'info');
    return;
  }
  state.downloadQueue = state.downloadQueue.filter(q => q.id !== id);
  renderQueuePanel();
}

export function clearFinished() {
  state.downloadQueue = state.downloadQueue.filter(q => q.status !== 'done');
  renderQueuePanel();
}

// ─── Rendering ────────────────────────────────────────────────────────────────
// No-ops if #download-queue-panel isn't mounted (user navigated away from
// Downloads) — the queue itself keeps processing in the background regardless.

export function renderQueuePanel() {
  updateRailBadge();
  const host = document.getElementById('download-queue-panel');
  if (!host) return;

  if (!state.downloadQueue.length) {
    host.innerHTML = '';
    return;
  }

  host.innerHTML = `
    <div class="dl-queue fade-in">
      <div class="dl-queue-header">
        <span class="dl-queue-title">Download queue</span>
        <button class="btn btn-outline" id="dl-queue-clear-done" style="font-size:11px;padding:5px 12px;">Clear finished</button>
      </div>
      <div class="dl-queue-list" id="dl-queue-list"></div>
    </div>`;

  const list = document.getElementById('dl-queue-list')!;
  state.downloadQueue.forEach(item => list.appendChild(renderQueueItem(item)));

  document.getElementById('dl-queue-clear-done')?.addEventListener('click', clearFinished);
}

function renderQueueItem(item: DownloadQueueItem): HTMLElement {
  const row = el('div', `dl-queue-item dl-queue-item--${item.status}`);
  const statusLabel = ({
    queued: 'Queued',
    downloading: item.attempts > 0 ? `Retrying (${item.attempts}/${MAX_RETRIES})…` : 'Downloading…',
    done: '✓ Done',
    failed: '✗ Failed',
  } as const)[item.status];

  const lines = stripAnsi(item.log);
  const lastLine = lines[lines.length - 1] ?? '';

  row.innerHTML = `
    <img class="dl-queue-cover" src="${item.cover}" alt="" loading="lazy" onerror="this.style.opacity=0.3"/>
    <div class="dl-queue-info">
      <div class="dl-queue-title-row">${esc(item.animeTitle)} <span class="dl-queue-ep">EP ${item.epNum}</span></div>
      <div class="dl-queue-status">${statusLabel}${item.status === 'downloading' && lastLine ? ` · ${esc(lastLine)}` : ''}</div>
    </div>
    <div class="dl-queue-actions">
      ${item.status === 'downloading' ? `<div class="spinner spinner-sm"></div>` : ''}
      ${item.log ? `<button class="btn-icon dl-queue-log-btn" title="View log">📄</button>` : ''}
      ${item.status === 'failed' ? `<button class="btn-icon dl-queue-retry-btn" title="Retry">↻</button>` : ''}
      ${item.status !== 'downloading' ? `<button class="btn-icon dl-queue-remove-btn" title="Remove">✕</button>` : ''}
    </div>`;

  row.querySelector('.dl-queue-log-btn')?.addEventListener('click', () => openLogModal(item));
  row.querySelector('.dl-queue-retry-btn')?.addEventListener('click', () => retryItem(item.id));
  row.querySelector('.dl-queue-remove-btn')?.addEventListener('click', () => removeItem(item.id));

  return row;
}

function openLogModal(item: DownloadQueueItem) {
  const modalTitle = document.querySelector('#modal-download .modal-title');
  if (modalTitle) modalTitle.textContent = `${item.animeTitle} — EP ${item.epNum}`;
  const log = document.getElementById('download-log')!;
  log.innerHTML = '';
  stripAnsi(item.log).forEach(fl => {
    const div = el('div');
    div.textContent = fl;
    log.appendChild(div);
  });
  log.scrollTop = log.scrollHeight;

  const status = document.getElementById('download-status');
  if (status) {
    const labels = { queued: 'Waiting to start…', downloading: 'Downloading…', done: '✓ Download complete!', failed: '✗ Download failed.' } as const;
    status.textContent = labels[item.status];
    status.style.color = item.status === 'done' ? 'var(--green)' : item.status === 'failed' ? 'var(--red)' : 'var(--text3)';
  }

  document.getElementById('modal-download')!.classList.add('open');
}

function updateRailBadge() {
  const badge = document.getElementById('rail-dl-badge');
  if (!badge) return;
  const active = state.downloadQueue.filter(q => q.status === 'queued' || q.status === 'downloading').length;
  if (active > 0) {
    badge.textContent = String(active);
    badge.style.display = 'flex';
  } else {
    badge.style.display = 'none';
  }
}
