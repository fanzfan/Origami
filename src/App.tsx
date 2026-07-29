import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import {
  api,
  ARCHIVE_PICK_EXTS,
  ArchiveInfo,
  DirListing,
  isArchive,
  isEditableFormat,
  makeDragImage,
  newJobId,
  Progress,
  splitParent,
} from "./api";
import { Welcome } from "./components/Welcome";
import { Browser } from "./components/Browser";
import { FileExplorer } from "./components/FileExplorer";
import { UiIcon } from "./icons";
import {
  CreateDialog,
  ExtractDialog,
  FileAssociations,
  ShellIntegration,
  PasswordManager,
  PasswordPrompt,
  PreviewModal,
  ProgressModal,
  SettingsDialog,
} from "./components/dialogs";
import { explainError } from "./i18n";
import { applySettings, applyWindowEffects, clampScale, loadSettings, saveSettings, SCALE_STEP } from "./settings";

export interface Toast {
  id: number;
  kind: "ok" | "error" | "info";
  text: string;
}

export interface JobState {
  jobId: string;
  title: string;
  progress: Progress | null;
}

function loadRecent(): string[] {
  try {
    return JSON.parse(localStorage.getItem("recent") ?? "[]");
  } catch {
    return [];
  }
}

export default function App() {
  const { t } = useTranslation();
  const [archivePath, setArchivePath] = useState<string | null>(null);
  const [info, setInfo] = useState<ArchiveInfo | null>(null);
  const [fsListing, setFsListing] = useState<DirListing | null>(null);
  const [fsLoading, setFsLoading] = useState(false);
  const [password, setPassword] = useState<string | undefined>();
  const [encoding, setEncoding] = useState("auto");
  const [loading, setLoading] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  // 当前所在的包内目录，由 Browser 上报；拖入文件时作为添加的目标目录。
  const [archiveCwd, setArchiveCwd] = useState("");
  const [recent, setRecent] = useState<string[]>(loadRecent);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [job, setJob] = useState<JobState | null>(null);

  // dialogs
  const [pwPrompt, setPwPrompt] = useState<{ resolve: (pw: string | null) => void } | null>(null);
  const [showPwMgr, setShowPwMgr] = useState(false);
  const [showFinder, setShowFinder] = useState(false);
  const [showAssoc, setShowAssoc] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [settings, setSettings] = useState(loadSettings);
  const [extractFor, setExtractFor] = useState<{ entries: string[] } | null>(null);
  const [createFor, setCreateFor] = useState<string[] | null>(null);
  const [previewPath, setPreviewPath] = useState<string | null>(null);

  // ----- settings: persist + apply (zoom + font)，外加平台习惯的快捷键 -----
  // Mac 用 Cmd，Windows/Linux 用 Ctrl，和绝大多数应用一致。
  const isMac = /Mac|iPhone|iPad|iPod/.test(navigator.platform) || /Mac OS X/.test(navigator.userAgent);
  // Windows 隐藏原生标题栏，窗口控制（仅关闭）内嵌到自绘标题栏。
  const isWin = navigator.userAgent.includes("Windows") || navigator.platform.startsWith("Win");
  useEffect(() => {
    applySettings(settings);
    saveSettings(settings);
  }, [settings]);

  // 窗口材质（亚克力 / 云母 / 毛玻璃）：仅主窗。材质、亚克力透明度，或影响玻璃
  // 色调的主题/深浅模式变化时都重新施加。
  useEffect(() => {
    applyWindowEffects(settings);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.material, settings.acrylicOpacity, settings.theme, settings.mode]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // 主修饰键：Mac=Cmd(meta)，其它平台=Ctrl
      const mod = isMac ? e.metaKey : e.ctrlKey;
      if (!mod || e.altKey) return;
      // Cmd+, / Ctrl+, 打开设置
      if (e.key === ",") {
        e.preventDefault();
        setShowSettings(true);
      } else if (e.key === "=" || e.key === "+") {
        e.preventDefault();
        setSettings((c) => ({ ...c, scale: clampScale(c.scale + SCALE_STEP) }));
      } else if (e.key === "-" || e.key === "_") {
        e.preventDefault();
        setSettings((c) => ({ ...c, scale: clampScale(c.scale - SCALE_STEP) }));
      } else if (e.key === "0") {
        e.preventDefault();
        setSettings((c) => ({ ...c, scale: 1 }));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [isMac]);

  const toastId = useRef(0);
  const toast = useCallback((kind: Toast["kind"], text: string) => {
    const id = ++toastId.current;
    setToasts((t) => [...t, { id, kind, text }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 4200);
  }, []);

  const addRecent = useCallback((path: string) => {
    setRecent((r) => {
      const next = [path, ...r.filter((x) => x !== path)].slice(0, 8);
      localStorage.setItem("recent", JSON.stringify(next));
      return next;
    });
  }, []);

  const askPassword = useCallback((): Promise<string | null> => {
    return new Promise((resolve) => setPwPrompt({ resolve }));
  }, []);

  const openArchive = useCallback(
    async (path: string, pw?: string, enc?: string) => {
      setLoading(true);
      try {
        const result = await api.listArchive(path, pw, enc ?? encoding);
        setArchivePath(path);
        setInfo(result);
        setPassword(result.usedPassword ?? pw);
        addRecent(path);
      } catch (e) {
        const msg = String(e);
        if (msg === "PASSWORD_REQUIRED") {
          const entered = await askPassword();
          if (entered) {
            await openArchive(path, entered, enc);
            return;
          }
          toast("info", t("app.info.encryptedOpenCancelled"));
        } else {
          toast("error", t("app.error.open", { message: explainError(msg) }));
        }
      } finally {
        setLoading(false);
      }
    },
    [encoding, addRecent, askPassword, toast, t],
  );

  const reopenWithEncoding = useCallback(
    async (enc: string) => {
      setEncoding(enc);
      if (archivePath) await openArchive(archivePath, password, enc);
    },
    [archivePath, password, openArchive],
  );

  const closeArchive = useCallback(() => {
    setArchivePath(null);
    setInfo(null);
    setPassword(undefined);
  }, []);

  // 进入/切换文件系统视图。path 省略 = 主目录；"" = 此电脑（驱动器列表）。
  const browseDir = useCallback(
    async (path?: string) => {
      setFsLoading(true);
      try {
        const listing = await api.listDir(path);
        setFsListing(listing);
      } catch (e) {
        toast("error", t("app.error.browseDirectory", { message: String(e) }));
      } finally {
        setFsLoading(false);
      }
    },
    [toast, t],
  );

  // 从压缩包路径栏跳到某个真实目录（离开压缩包，进入文件系统视图）。"" = 此电脑。
  const navigateFs = useCallback(
    (path: string) => {
      closeArchive();
      browseDir(path);
    },
    [closeArchive, browseDir],
  );

  // ----- progress events -----
  const jobRef = useRef<JobState | null>(null);
  jobRef.current = job;
  useEffect(() => {
    const un = api.onProgress((p) => {
      const cur = jobRef.current;
      if (cur && cur.jobId === p.jobId) {
        setJob({ ...cur, progress: p });
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // ----- file drop & open-with -----
  /** 落点为「初始界面 / 文件系统视图」时的规则：单个压缩包 → 打开；其余 → 新建压缩包。
   *  扩展名看不出来时再按内容确认一次，免得把没后缀 / 后缀写错的压缩包当成待压缩文件。 */
  const openOrCompress = useCallback(
    async (paths: string[]) => {
      if (paths.length === 0) return;
      if (paths.length === 1) {
        if (isArchive(paths[0])) {
          openArchive(paths[0]);
          return;
        }
        const probed = await api.probeArchive(paths[0]).catch(() => null);
        if (probed) {
          openArchive(paths[0]);
          return;
        }
      }
      setCreateFor(paths);
    },
    [openArchive],
  );

  // 真正的落点处理在 addSources 之后定义；这里用 ref 转发，避免监听器随状态反复重订阅
  // （unlisten 是异步的，重订阅期间的拖放可能丢失）。
  const dropRef = useRef<(paths: string[]) => void>(() => {});
  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "over") setDragOver(true);
      else if (e.payload.type === "drop") {
        setDragOver(false);
        dropRef.current(e.payload.paths);
      } else setDragOver(false);
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // ----- Finder 右键 / 双击打开（深链 + 启动前积压的动作）-----
  /** 返回是否成功（取消视为成功，不需要回到主窗口报错）。 */
  const quickCreate = useCallback(
    async (format: string, sources: string[], useMini: boolean) => {
      const jobId = newJobId();
      try {
        const dest = await api.defaultCreateDest(sources, format);
        if (!useMini) setJob({ jobId, title: t("app.status.compressing"), progress: null });
        const out = await api.createArchive({
          jobId,
          dest,
          sources,
          format,
          level: settings.level,
          excludeJunk: settings.excludeJunk,
        });
        toast("ok", t("app.success.compressedFile", { name: out.split(/[\\/]/).pop() }));
        return true;
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") {
          toast("info", t("common.cancelled"));
          return true;
        }
        toast("error", t("app.error.compress", { message: explainError(msg) }));
        return false;
      } finally {
        if (!useMini) setJob(null);
      }
    },
    [toast, settings.level, settings.excludeJunk, t],
  );

  /** 快捷解压（主窗可见时用应用内进度）。返回是否成功（取消视为成功）。 */
  const quickExtract = useCallback(
    async (mode: string, path: string, useMini: boolean) => {
      const jobId = newJobId();
      try {
        const dest = await api.quickExtractDest(path, mode);
        if (!useMini) setJob({ jobId, title: t("app.status.extracting"), progress: null });
        // "smart" → 智能解压（多个顶层文件自动套一层文件夹）；"here"/"folder" → 原样。
        const out = await api.extractArchive({ jobId, path, dest, smart: mode === "smart" });
        toast("ok", t("app.success.extractedFile", { name: out.split(/[\\/]/).pop() }));
        if (settings.openAfterExtract) openPath(out).catch(() => {});
        return true;
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") {
          toast("info", t("common.cancelled"));
          return true;
        }
        if (msg === "PASSWORD_REQUIRED") {
          // 加密归档：主窗可见，直接打开它，让用户输入密码后在应用内解压。
          openArchive(path);
          return true;
        }
        toast("error", t("app.error.extract", { message: explainError(msg) }));
        return false;
      } finally {
        if (!useMini) setJob(null);
      }
    },
    [toast, settings.openAfterExtract, openArchive, t],
  );

  const draining = useRef(false);
  const drainActions = useCallback(async () => {
    if (draining.current) return; // 进行中的循环会在下一轮取走新动作
    // 冷启动快捷压缩时主窗是隐藏的——此时由「可见」的迷你窗驱动并显示进度，
    // 主窗不参与，以免和迷你窗争抢同一批动作（takePendingActions 是破坏性取走）。
    // 主窗可见（应用已打开时右键压缩 / 打开归档）才在此处理，用应用内进度。
    if (!(await getCurrentWindow().isVisible().catch(() => true))) return;
    draining.current = true;
    try {
      for (;;) {
        const actions = await api.takePendingActions();
        if (actions.length === 0) break;
        const quickCreates: { format: string; paths: string[] }[] = [];
        const quickExtracts: { mode: string; path: string }[] = [];
        for (const a of actions) {
          if (a.kind === "open") {
            // 单个归档 → 打开；其它（普通文件 / 文件夹 / 多个文件，例如拖到 Dock 图标
            // 或 Windows 快捷方式上）→ 进入压缩。这里不走「添加到已打开的包」：从外壳
            // 触发的打开动作，落到当前包里会很意外。
            openOrCompress(a.paths);
          } else if (a.kind === "create") {
            // ask（详细设置）已由 ask 小窗处理，不会进到这里；其余为快捷压缩。
            quickCreates.push(a);
          } else if (a.kind === "extract") {
            // ask（解压到…）已由 ask 小窗处理，不会进到这里；其余逐个快捷解压。
            for (const p of a.paths) quickExtracts.push({ mode: a.mode, path: p });
          }
        }
        if (quickCreates.length > 0 || quickExtracts.length > 0) {
          const useMini = await api.beginQuickJob();
          let ok = true;
          for (const a of quickCreates) {
            ok = (await quickCreate(a.format, a.paths, useMini)) && ok;
          }
          for (const e of quickExtracts) {
            ok = (await quickExtract(e.mode, e.path, useMini)) && ok;
          }
          if (useMini) {
            // 让迷你窗口的 100% 状态停留一瞬
            await new Promise((r) => setTimeout(r, 800));
            await api.endQuickJob(ok);
          }
        }
      }
    } finally {
      draining.current = false;
    }
  }, [openOrCompress, quickCreate, quickExtract]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      unlisten = await api.onDeepLink(drainActions);
      // 先让 frontend_ready 决定是否显示主窗（非快捷启动会显示），再 drain：
      // drainActions 仅在主窗可见时驱动，顺序保证可见后才取动作，避免漏处理。
      await api.frontendReady();
      await drainActions();
    })();
    return () => unlisten?.();
  }, [drainActions]);

  // ----- actions -----
  const pickArchive = useCallback(async () => {
    const sel = await openDialog({
      multiple: false,
      filters: [{ name: t("app.dialog.archiveFilter"), extensions: ARCHIVE_PICK_EXTS }],
    });
    if (typeof sel === "string") openArchive(sel);
  }, [openArchive, t]);

  const pickFilesToCompress = useCallback(async () => {
    const sel = await openDialog({ multiple: true });
    if (Array.isArray(sel) && sel.length > 0) setCreateFor(sel);
    else if (typeof sel === "string") setCreateFor([sel]);
  }, []);

  const pickFolderToCompress = useCallback(async () => {
    const sel = await openDialog({ multiple: false, directory: true });
    if (typeof sel === "string") setCreateFor([sel]);
  }, []);

  const runExtract = useCallback(
    async (dest: string, smart: boolean, entries: string[]) => {
      if (!archivePath) return;
      const jobId = newJobId();
      setExtractFor(null);
      setJob({ jobId, title: t("app.status.extracting"), progress: null });
      try {
        const out = await api.extractArchive({
          jobId,
          path: archivePath,
          dest,
          password,
          encoding,
          entries,
          smart,
        });
        toast("ok", t("app.success.extracted"));
        if (settings.openAfterExtract) openPath(out).catch(() => {});
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") toast("info", t("common.cancelled"));
        else if (msg === "PASSWORD_REQUIRED") {
          // 先收起进度层再显示密码提示；两者同为模态层时，保留 job 会遮住密码输入框。
          setJob(null);
          const pw = await askPassword();
          if (pw) {
            setPassword(pw);
            // retry with new password
            const retryId = newJobId();
            setJob({ jobId: retryId, title: t("app.status.extracting"), progress: null });
            try {
              const out = await api.extractArchive({
                jobId: retryId,
                path: archivePath,
                dest,
                password: pw,
                encoding,
                entries,
                smart,
              });
              toast("ok", t("app.success.extracted"));
              if (settings.openAfterExtract) openPath(out).catch(() => {});
            } catch (e2) {
              toast("error", t("app.error.extract", { message: String(e2) }));
            }
          }
        } else toast("error", t("app.error.extract", { message: explainError(msg) }));
      } finally {
        setJob(null);
      }
    },
    [archivePath, password, encoding, toast, askPassword, settings.openAfterExtract, t],
  );

  const runCreate = useCallback(
    async (p: { dest: string; format: string; level: number; method?: string; password?: string; volumeSize?: number }) => {
      const sources = createFor!;
      const jobId = newJobId();
      setCreateFor(null);
      setJob({ jobId, title: t("app.status.compressing"), progress: null });
      try {
        await api.createArchive({ jobId, sources, excludeJunk: settings.excludeJunk, ...p });
        toast("ok", t("app.success.compressed"));
        // 文件系统视图下刷新当前目录，让新建的压缩包立即出现。
        if (fsListing) browseDir(fsListing.path);
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") toast("info", t("common.cancelled"));
        else toast("error", t("app.error.compress", { message: explainError(msg) }));
      } finally {
        setJob(null);
      }
    },
    [createFor, toast, settings.excludeJunk, fsListing, browseDir, t],
  );

  /** 把真实路径添加进当前打开的压缩包的 dir 目录。 */
  const addSources = useCallback(
    async (sources: string[], dir: string) => {
      if (!archivePath) return;
      // 把包自己拖进自己会无限套娃，直接剔除。
      const list = sources.filter((s) => s !== archivePath);
      if (list.length === 0) return;
      const jobId = newJobId();
      setJob({ jobId, title: t("app.status.adding"), progress: null });
      try {
        await api.archiveAdd({ jobId, path: archivePath, sources: list, dir, password, encoding });
        toast("ok", t("app.success.added", { count: list.length }));
        await openArchive(archivePath, password, encoding);
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") toast("info", t("common.cancelled"));
        else toast("error", t("app.error.add", { message: explainError(msg) }));
      } finally {
        setJob(null);
      }
    },
    [archivePath, password, encoding, openArchive, toast, t],
  );

  const runAdd = useCallback(
    async (dir: string) => {
      if (!archivePath) return;
      const sel = await openDialog({ multiple: true, title: t("app.dialog.addFiles") });
      const sources = Array.isArray(sel) ? sel : typeof sel === "string" ? [sel] : [];
      if (sources.length > 0) await addSources(sources, dir);
    },
    [archivePath, addSources, t],
  );

  // 拖入应用：已打开可编辑的压缩包 → 添加到当前包内目录；否则按初始界面的规则处理。
  // 只读格式（RAR 等）无法添加，退回「打开 / 新建压缩包」，不至于让拖放无声失败。
  const addOnDrop = !!(archivePath && info && isEditableFormat(info.format) && !info.volumes);
  const handleDrop = useCallback(
    (paths: string[]) => {
      if (paths.length === 0) return;
      if (addOnDrop) addSources(paths, archiveCwd);
      else openOrCompress(paths);
    },
    [addOnDrop, addSources, archiveCwd, openOrCompress],
  );
  dropRef.current = handleDrop;

  // 从压缩包内拖到应用外：先把选中条目解压到暂存区，再发起系统原生拖放。
  // 暂存区按包指纹缓存，同一批条目再次拖出几乎瞬时。
  const dragOutEntries = useCallback(
    async (entries: string[]) => {
      if (!archivePath || entries.length === 0) return;
      const label =
        entries.length === 1
          ? entries[0].replace(/\/+$/, "").split("/").pop()!
          : t("app.drag.items", { count: entries.length });
      try {
        const started = performance.now();
        const staged = await api.stageEntriesForDrag({
          jobId: newJobId(),
          path: archivePath,
          entries,
          password,
          encoding,
        });
        const slow = performance.now() - started > 500;
        const result = await api.startNativeDrag(staged, makeDragImage(label));
        // 解压耗时较久时鼠标多半已经松开，拖放起不来。此时暂存区已就绪，再拖一次即可。
        if (slow && result === "Cancel") toast("info", t("app.info.dragReady"));
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") return;
        if (msg === "PASSWORD_REQUIRED") {
          // 拖放动作此时已经结束，无法接着拖；记住密码并提示重拖一次。
          const pw = await askPassword();
          if (pw) {
            setPassword(pw);
            toast("info", t("app.info.dragPasswordSaved"));
          }
          return;
        }
        toast("error", t("app.error.dragOut", { message: explainError(msg) }));
      }
    },
    [archivePath, password, encoding, askPassword, toast, t],
  );

  // 文件系统视图里拖出：已经是真实文件，直接发起原生拖放。
  const dragOutFiles = useCallback(
    (paths: string[]) => {
      if (paths.length === 0) return;
      const label =
        paths.length === 1 ? splitParent(paths[0]).name : t("app.drag.items", { count: paths.length });
      api.startNativeDrag(paths, makeDragImage(label)).catch((e) => {
        toast("error", t("app.error.dragOut", { message: String(e) }));
      });
    },
    [toast, t],
  );

  const runRemove = useCallback(
    async (entries: string[]) => {
      if (!archivePath || entries.length === 0) return;
      const jobId = newJobId();
      setJob({ jobId, title: t("app.status.removing"), progress: null });
      try {
        await api.archiveRemove({ jobId, path: archivePath, entries, password, encoding });
        toast("ok", t("app.success.removed", { count: entries.length }));
        await openArchive(archivePath, password, encoding);
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") toast("info", t("common.cancelled"));
        else toast("error", t("app.error.remove", { message: explainError(msg) }));
      } finally {
        setJob(null);
      }
    },
    [archivePath, password, encoding, openArchive, toast, t],
  );

  const openExternal = useCallback(
    async (entryPath: string) => {
      if (!archivePath) return;
      try {
        const file = await api.extractEntryToTemp({
          path: archivePath,
          entry: entryPath,
          password,
          encoding,
        });
        await openPath(file);
      } catch (e) {
        const msg = String(e);
        if (msg === "PASSWORD_REQUIRED") {
          const pw = await askPassword();
          if (pw) {
            setPassword(pw);
            try {
              const file = await api.extractEntryToTemp({ path: archivePath, entry: entryPath, password: pw, encoding });
              await openPath(file);
            } catch (e2) {
              toast("error", t("app.error.open", { message: String(e2) }));
            }
          }
        } else toast("error", t("app.error.open", { message: explainError(msg) }));
      }
    },
    [archivePath, password, encoding, askPassword, toast, t],
  );

  const runTest = useCallback(async () => {
    if (!archivePath) return;
    const jobId = newJobId();
    setJob({ jobId, title: t("app.status.verifying"), progress: null });
    try {
      await api.testArchive(jobId, archivePath, password);
      toast("ok", t("app.success.verified"));
    } catch (e) {
      const msg = String(e);
      if (msg === "CANCELLED") toast("info", t("common.cancelled"));
      else toast("error", t("app.error.verify", { message: explainError(msg) }));
    } finally {
      setJob(null);
    }
  }, [archivePath, password, toast, t]);

  const cancelJob = useCallback(() => {
    if (job) api.cancelJob(job.jobId);
  }, [job]);

  const title = useMemo(() => {
    if (archivePath) {
      const name = archivePath.split(/[\\/]/).pop();
      return info ? `${name} — ${info.format}` : name ?? "";
    }
    if (fsListing) return fsListing.path || t("app.title.thisComputer");
    return "Origami";
  }, [archivePath, info, fsListing, t]);

  return (
    <div className="app">
      <div className={`titlebar${isWin ? " win" : ""}`} data-tauri-drag-region>
        <span className="brand-logo titlebar-logo" data-tauri-drag-region aria-hidden="true" />
        <span className="title" data-tauri-drag-region>{title}</span>
        <span style={{ flex: 1 }} data-tauri-drag-region />
        {archivePath && (
          <button className="btn ghost sm" onClick={closeArchive}>
            <UiIcon name="close" />
            {t("app.title.closeArchive")}
          </button>
        )}
        {!archivePath && fsListing && (
          <button className="btn ghost sm" onClick={() => setFsListing(null)} title={t("app.title.returnHome")}>
            <UiIcon name="home" />
            {t("app.title.home")}
          </button>
        )}
        <button className="btn ghost sm titlebar-tool" onClick={() => setShowFinder(true)} title={t("app.title.shellIntegration")} aria-label={t("app.title.shellIntegration")}>
          <UiIcon name="integration" />
        </button>
        <button className="btn ghost sm titlebar-tool" onClick={() => setShowAssoc(true)} title={t("app.title.fileAssociations")} aria-label={t("app.title.fileAssociations")}>
          <UiIcon name="link" />
        </button>
        <button className="btn ghost sm titlebar-tool" onClick={() => setShowPwMgr(true)} title={t("app.title.passwordManager")} aria-label={t("app.title.passwordManager")}>
          <UiIcon name="key" />
        </button>
        <button className="btn ghost sm titlebar-tool" onClick={() => setShowSettings(true)} title={t(isMac ? "app.title.settingsShortcutMac" : "app.title.settingsShortcutOther")} aria-label={t("app.title.settings")}>
          <UiIcon name="settings" />
        </button>
        {isWin && (
          <button className="btn ghost sm titlebar-tool win-close" onClick={() => getCurrentWindow().close()} title={t("common.close")} aria-label={t("app.title.closeWindow")}>
            <UiIcon name="close" />
          </button>
        )}
      </div>

      {info && archivePath ? (
        // key：换包时重置浏览器内部状态（当前目录、选中项），避免停留在上一个包的路径。
        <Browser
          key={archivePath}
          info={info}
          archivePath={archivePath}
          loading={loading}
          encoding={encoding}
          onEncodingChange={reopenWithEncoding}
          onExtract={(entries) => setExtractFor({ entries })}
          onTest={runTest}
          onPreview={(p) => setPreviewPath(p)}
          onOpenExternal={openExternal}
          onAdd={runAdd}
          onRemove={runRemove}
          onDragOut={dragOutEntries}
          onCwdChange={setArchiveCwd}
          onNavigateFs={navigateFs}
        />
      ) : fsListing ? (
        <FileExplorer
          listing={fsListing}
          loading={fsLoading}
          onNavigate={(path) => browseDir(path)}
          onOpenArchive={openArchive}
          onOpenFile={(path) => openPath(path).catch((e) => toast("error", t("app.error.open", { message: String(e) })))}
          onCompress={(paths) => setCreateFor(paths)}
          onDragOut={dragOutFiles}
        />
      ) : (
        <Welcome
          dragOver={dragOver}
          loading={loading}
          recent={recent}
          onOpen={pickArchive}
          onOpenRecent={openArchive}
          onCompressFiles={pickFilesToCompress}
          onCompressFolder={pickFolderToCompress}
          onBrowse={() => browseDir()}
        />
      )}

      {/* 拖入提示：初始界面自带拖放区，这里只覆盖压缩包 / 文件系统视图。 */}
      {dragOver && (archivePath || fsListing) && (
        <div className="drop-overlay">
          <div className="drop-overlay-card">
            <UiIcon name={addOnDrop ? "add" : "folder-archive"} size={30} />
            <p>
              {addOnDrop
                ? t("app.drop.addTo", { name: splitParent(archivePath!).name })
                : t("app.drop.compress")}
            </p>
          </div>
        </div>
      )}

      {extractFor && archivePath && (
        <ExtractDialog
          archivePath={archivePath}
          entryCount={extractFor.entries.length}
          onCancel={() => setExtractFor(null)}
          onConfirm={(dest, smart) => runExtract(dest, smart, extractFor.entries)}
          pickDir={async () => {
            const sel = await openDialog({ directory: true, multiple: false });
            return typeof sel === "string" ? sel : null;
          }}
          defaultDir={() => api.defaultExtractDir(archivePath)}
        />
      )}

      {createFor && (
        <CreateDialog
          sources={createFor}
          defaultLevel={settings.level}
          onCancel={() => setCreateFor(null)}
          onConfirm={runCreate}
          pickDest={async (defName: string, ext: string) => {
            const sel = await saveDialog({
              defaultPath: defName + "." + ext,
              filters: [{ name: ext.toUpperCase(), extensions: [ext.split(".").pop()!] }],
            });
            return sel ?? null;
          }}
        />
      )}

      {pwPrompt && (
        <PasswordPrompt
          onSubmit={(pw) => {
            pwPrompt.resolve(pw);
            setPwPrompt(null);
          }}
        />
      )}

      {showPwMgr && <PasswordManager onClose={() => setShowPwMgr(false)} />}

      {showFinder && <ShellIntegration onClose={() => setShowFinder(false)} toast={toast} />}

      {showAssoc && <FileAssociations onClose={() => setShowAssoc(false)} toast={toast} />}

      {showSettings && (
        <SettingsDialog
          settings={settings}
          onChange={(patch) => setSettings((c) => ({ ...c, ...patch }))}
          onClose={() => setShowSettings(false)}
        />
      )}

      {previewPath && archivePath && (
        <PreviewModal
          archivePath={archivePath}
          entryPath={previewPath}
          password={password}
          encoding={encoding}
          onClose={() => setPreviewPath(null)}
        />
      )}

      {job && <ProgressModal job={job} onCancel={cancelJob} />}

      <div className="toasts">
        {toasts.map((t) => (
          <div key={t.id} className={`toast ${t.kind}`}>
            {t.text}
          </div>
        ))}
      </div>
    </div>
  );
}
