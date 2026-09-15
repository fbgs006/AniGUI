// ─── Runtime Setup (auto-bootstrap ani-cli / mpv / fzf / Git Bash) ────────────
// Downloads a self-contained runtime into %APPDATA%/AniGUI/runtime/ so the
// user never has to install ani-cli/mpv/Git Bash themselves. See
// src-tauri/src/bootstrap.rs for the backend side of this flow.

import './runtime-setup.css';

import { invoke } from '@tauri-apps/api/core';
import type { RuntimeComponent, RuntimeInstallProgress, RuntimeStatus } from '../types';
import { toast } from './toast';

const LABELS: Record<RuntimeComponent, string> = {
  "git-bash": "Git Bash",
  "mpv": "mpv Player",
  "fzf": "fzf",
  "ani-cli": "ani-cli",
};

let installing = false;

function row(component: RuntimeComponent): HTMLElement | null {
  return document.querySelector(`.runtime-row[data-component="${component}"]`);
}

function resetRows() {
  for (const component of Object.keys(LABELS) as RuntimeComponent[]) {
    setRowStatus(component, "Waiting…", 0);
  }
}

function setRowStatus(component: RuntimeComponent, statusText: string, percent: number, indeterminate = false) {
  const el = row(component);
  if (!el) return;
  el.classList.remove("done", "error");
  const statusEl = el.querySelector('[data-role="status"]');
  const fillEl = el.querySelector('[data-role="fill"]') as HTMLElement | null;
  if (statusEl) statusEl.textContent = statusText;
  if (fillEl) {
    fillEl.classList.toggle("indeterminate", indeterminate);
    if (!indeterminate) fillEl.style.width = `${percent}%`;
  }
}

function formatBytes(n: number): string {
  if (n <= 0) return "";
  const mb = n / (1024 * 1024);
  return mb >= 1 ? `${mb.toFixed(1)} MB` : `${(n / 1024).toFixed(0)} KB`;
}

export function handleRuntimeProgress(payload: RuntimeInstallProgress) {
  const { component, phase, bytes, total, message } = payload;
  const el = row(component);
  if (!el) return;

  switch (phase) {
    case "downloading": {
      const percent = total > 0 ? Math.min(100, (bytes / total) * 100) : 0;
      const label = total > 0 ? `${formatBytes(bytes)} / ${formatBytes(total)}` : message;
      setRowStatus(component, label, percent, total === 0);
      break;
    }
    case "extracting":
    case "installing":
      setRowStatus(component, message, 0, true);
      break;
    case "done":
      el.classList.add("done");
      setRowStatus(component, message || "Ready", 100);
      break;
    case "error":
      el.classList.add("error");
      setRowStatus(component, message || "Failed", 0);
      break;
  }
}

export function openRuntimeSetup(_status: RuntimeStatus) {
  resetRows();
  const skipBtn = document.getElementById("runtime-setup-skip") as HTMLButtonElement | null;
  if (skipBtn) skipBtn.style.display = "";
  document.getElementById("modal-runtime-setup")?.classList.add("open");
}

function closeRuntimeSetup() {
  document.getElementById("modal-runtime-setup")?.classList.remove("open");
}

async function runInstall(force: boolean) {
  if (installing) return;
  installing = true;
  const installBtn = document.getElementById("runtime-setup-install") as HTMLButtonElement | null;
  const skipBtn = document.getElementById("runtime-setup-skip") as HTMLButtonElement | null;
  if (installBtn) { installBtn.disabled = true; installBtn.textContent = "Installing…"; }
  if (skipBtn) skipBtn.disabled = true;

  try {
    await invoke("install_runtime", { force });
    toast("AniGUI is ready to go!", "success");
    closeRuntimeSetup();
  } catch (err: any) {
    toast(`Setup failed: ${err}`, "error");
  } finally {
    installing = false;
    if (installBtn) { installBtn.disabled = false; installBtn.textContent = "Install Now"; }
    if (skipBtn) skipBtn.disabled = false;
  }
}

/** Reopens the setup modal for a forced reinstall, triggered from Settings. */
export async function repairRuntime() {
  resetRows();
  const skipBtn = document.getElementById("runtime-setup-skip") as HTMLButtonElement | null;
  if (skipBtn) skipBtn.style.display = "none";
  document.getElementById("modal-runtime-setup")?.classList.add("open");
  await runInstall(true);
}

export function wireRuntimeSetup() {
  document.getElementById("runtime-setup-install")?.addEventListener("click", () => runInstall(false));
  document.getElementById("runtime-setup-skip")?.addEventListener("click", async () => {
    await invoke("skip_runtime_setup");
    closeRuntimeSetup();
  });
}
