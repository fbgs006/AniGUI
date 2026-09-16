// ─── AniGUI Shared Types ─────────────────────────────────────────────────────

export interface MediaTitle { romaji: string; english?: string; }
export interface CoverImage { medium: string; large: string; }
export interface MediaListEntry { id: number; progress: number; status: string; }

export interface RelatedNode {
  id: number;
  title: MediaTitle;
  coverImage: { medium: string };
  type: string;
  format: string;
}

export interface RelationEdge {
  relationType: string;
  node: RelatedNode;
}

export interface NextAiringEpisode {
  airingAt: number; // unix seconds
  episode: number;
  timeUntilAiring: number; // seconds from now
}

export interface Media {
  id: number;
  idMal?: number;
  title: MediaTitle;
  format?: string;
  episodes?: number;
  averageScore?: number;
  status: string;
  season?: string;
  seasonYear?: number;
  genres: string[];
  description?: string;
  coverImage: CoverImage;
  bannerImage?: string;
  mediaListEntry?: MediaListEntry;
  relations?: { edges: RelationEdge[] };
  nextAiringEpisode?: NextAiringEpisode;
}

export interface Config {
  bash_path: string;
  quality: string;
  confirm_before_sync: boolean;
  anilist_token: string;
  download_dir: string;
  theme: string;
  auto_sync: boolean;
  dub: boolean;
  skip_auto_setup: boolean;
}

export interface RuntimeStatus {
  bash_present: boolean;
  mpv_present: boolean;
  fzf_present: boolean;
  anicli_present: boolean;
  all_present: boolean;
  system_fallback_present: boolean;
  skip_auto_setup: boolean;
}

export type RuntimeComponent = "git-bash" | "mpv" | "fzf" | "ani-cli";
export type RuntimeInstallPhase = "downloading" | "extracting" | "installing" | "done" | "error";

export interface RuntimeInstallProgress {
  component: RuntimeComponent;
  phase: RuntimeInstallPhase;
  bytes: number;
  total: number;
  message: string;
}

export interface HistoryEntry {
  animeId: number;
  animeTitle: string;
  cover: string;
  epNum: number;
  watchedAt: number; // Unix timestamp in seconds
}

export type TabName = "continue" | "trending" | "search" | "planning" | "downloads";

export type DownloadQueueStatus = "queued" | "downloading" | "done" | "failed";

export interface DownloadQueueItem {
  id: string;
  animeId: number;
  animeTitle: string;
  cover: string;
  epNum: number;
  status: DownloadQueueStatus;
  attempts: number;
  log: string;
}
