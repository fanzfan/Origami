import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

export interface Entry {
  path: string;
  size: number;
  compressed: number;
  isDir: boolean;
  mtime: number | null;
  encrypted: boolean;
  crc: number | null;
}

export interface ArchiveInfo {
  format: string;
  entries: Entry[];
  hasEncrypted: boolean;
  totalSize: number;
  totalCompressed: number;
  comment: string | null;
  usedPassword: string | null;
}

export interface Progress {
  jobId: string;
  current: number;
  total: number;
  file: string;
  done: boolean;
}

export interface Preview {
  kind: "text" | "image" | "binary";
  text: string | null;
  data: string | null;
  mime: string | null;
  truncated: boolean;
  size: number;
}

export interface SavedPassword {
  password: string;
  label: string | null;
  added_at: number;
  last_used: number | null;
}

export const api = {
  listArchive: (path: string, password?: string, encoding?: string) =>
    invoke<ArchiveInfo>("list_archive", { path, password, encoding }),

  extractArchive: (p: {
    jobId: string;
    path: string;
    dest: string;
    password?: string;
    encoding?: string;
    entries?: string[];
    smart?: boolean;
  }) => invoke<string>("extract_archive", { ...p }),

  createArchive: (p: {
    jobId: string;
    dest: string;
    sources: string[];
    format: string;
    level: number;
    method?: string;
    password?: string;
    volumeSize?: number;
    excludeJunk?: boolean;
  }) => invoke<string>("create_archive", { ...p }),

  archiveAdd: (p: {
    jobId: string;
    path: string;
    sources: string[];
    dir?: string;
    password?: string;
    encoding?: string;
  }) => invoke<void>("archive_add", { ...p }),

  archiveRemove: (p: {
    jobId: string;
    path: string;
    entries: string[];
    password?: string;
    encoding?: string;
  }) => invoke<void>("archive_remove", { ...p }),

  testArchive: (jobId: string, path: string, password?: string) =>
    invoke<void>("test_archive", { jobId, path, password }),

  previewEntry: (path: string, entry: string, password?: string, encoding?: string) =>
    invoke<Preview>("preview_entry", { path, entry, password, encoding }),

  cancelJob: (jobId: string) => invoke<void>("cancel_job", { jobId }),

  systemIcon: (ext: string, isDir: boolean) =>
    invoke<string | null>("system_icon", { ext, isDir }),

  pwList: () => invoke<SavedPassword[]>("pw_list"),
  pwAdd: (password: string, label?: string) => invoke<void>("pw_add", { password, label }),
  pwRemove: (password: string) => invoke<void>("pw_remove", { password }),

  systemAuthAvailable: () => invoke<boolean>("system_auth_available"),
  systemAuth: (reason: string) => invoke<boolean>("system_auth", { reason }),
  extractEntryToTemp: (p: { path: string; entry: string; password?: string; encoding?: string }) =>
    invoke<string>("extract_entry_to_temp", { ...p }),

  defaultExtractDir: (path: string) => invoke<string>("default_extract_dir", { path }),
  defaultCreateDest: (sources: string[], ext: string) =>
    invoke<string>("default_create_dest", { sources, ext }),

  takePendingActions: () => invoke<PendingAction[]>("take_pending_actions"),
  frontendReady: () => invoke<void>("frontend_ready"),
  beginQuickJob: () => invoke<boolean>("begin_quick_job"),
  endQuickJob: (ok: boolean) => invoke<void>("end_quick_job", { ok }),
  installShellMenu: () => invoke<void>("install_shell_menu"),
  uninstallShellMenu: () => invoke<void>("uninstall_shell_menu"),
  shellMenuInstalled: () => invoke<boolean>("shell_menu_installed"),
  appPlatform: () => invoke<string>("app_platform"),

  fileAssocSupported: () => invoke<boolean>("file_assoc_supported"),
  fileAssocList: () => invoke<AssocEntry[]>("file_assoc_list"),
  fileAssocSet: (exts: string[], associate: boolean) =>
    invoke<void>("file_assoc_set", { exts, associate }),

  onProgress: (cb: (p: Progress) => void): Promise<UnlistenFn> =>
    listen<Progress>("job-progress", (e) => cb(e.payload)),

  onDeepLink: (cb: () => void): Promise<UnlistenFn> =>
    listen("deep-link-available", () => cb()),
};

export interface AssocEntry {
  ext: string;
  associated: boolean;
  currentApp: string | null;
}

export type PendingAction =
  | { kind: "open"; paths: string[] }
  | { kind: "create"; format: string; paths: string[] };

export function fmtSize(n: number): string {
  if (n === 0) return "0 B";
  const u = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log2(n) / 10), u.length - 1);
  const v = n / 2 ** (10 * i);
  return `${v >= 100 || i === 0 ? Math.round(v) : v.toFixed(1)} ${u[i]}`;
}

export function fmtDate(ts: number | null): string {
  if (!ts) return "—";
  const d = new Date(ts * 1000);
  const p = (x: number) => String(x).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function newJobId(): string {
  return Math.random().toString(36).slice(2) + Date.now().toString(36);
}
