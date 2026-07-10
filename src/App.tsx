import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { api, ARCHIVE_EXTS, ArchiveInfo, DirListing, isArchive, newJobId, Progress } from "./api";
import { Welcome } from "./components/Welcome";
import { Browser } from "./components/Browser";
import { FileExplorer } from "./components/FileExplorer";
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
  const [archivePath, setArchivePath] = useState<string | null>(null);
  const [info, setInfo] = useState<ArchiveInfo | null>(null);
  const [fsListing, setFsListing] = useState<DirListing | null>(null);
  const [fsLoading, setFsLoading] = useState(false);
  const [password, setPassword] = useState<string | undefined>();
  const [encoding, setEncoding] = useState("auto");
  const [loading, setLoading] = useState(false);
  const [dragOver, setDragOver] = useState(false);
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
          toast("info", "已取消打开加密归档");
        } else {
          toast("error", `打开失败：${msg}`);
        }
      } finally {
        setLoading(false);
      }
    },
    [encoding, addRecent, askPassword, toast],
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
        toast("error", `无法打开目录：${e}`);
      } finally {
        setFsLoading(false);
      }
    },
    [toast],
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
  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "over") setDragOver(true);
      else if (e.payload.type === "drop") {
        setDragOver(false);
        const paths = e.payload.paths;
        if (paths.length === 1 && isArchive(paths[0])) {
          openArchive(paths[0]);
        } else if (paths.length > 0) {
          setCreateFor(paths);
        }
      } else setDragOver(false);
    });
    return () => {
      un.then((f) => f());
    };
  }, [openArchive]);

  // ----- Finder 右键 / 双击打开（深链 + 启动前积压的动作）-----
  /** 返回是否成功（取消视为成功，不需要回到主窗口报错）。 */
  const quickCreate = useCallback(
    async (format: string, sources: string[], useMini: boolean) => {
      const jobId = newJobId();
      try {
        const dest = await api.defaultCreateDest(sources, format);
        if (!useMini) setJob({ jobId, title: "正在压缩…", progress: null });
        const out = await api.createArchive({
          jobId,
          dest,
          sources,
          format,
          level: settings.level,
          excludeJunk: settings.excludeJunk,
        });
        toast("ok", `压缩完成：${out.split("/").pop()}`);
        return true;
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") {
          toast("info", "已取消");
          return true;
        }
        toast("error", `压缩失败：${msg}`);
        return false;
      } finally {
        if (!useMini) setJob(null);
      }
    },
    [toast, settings.level, settings.excludeJunk],
  );

  /** 快捷解压（主窗可见时用应用内进度）。返回是否成功（取消视为成功）。 */
  const quickExtract = useCallback(
    async (mode: string, path: string, useMini: boolean) => {
      const jobId = newJobId();
      try {
        const dest = await api.quickExtractDest(path, mode);
        if (!useMini) setJob({ jobId, title: "正在解压…", progress: null });
        // "smart" → 智能解压（多个顶层文件自动套一层文件夹）；"here"/"folder" → 原样。
        const out = await api.extractArchive({ jobId, path, dest, smart: mode === "smart" });
        toast("ok", `解压完成：${out.split(/[\\/]/).pop()}`);
        if (settings.openAfterExtract) openPath(out).catch(() => {});
        return true;
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") {
          toast("info", "已取消");
          return true;
        }
        if (msg === "PASSWORD_REQUIRED") {
          // 加密归档：主窗可见，直接打开它，让用户输入密码后在应用内解压。
          openArchive(path);
          return true;
        }
        toast("error", `解压失败：${msg}`);
        return false;
      } finally {
        if (!useMini) setJob(null);
      }
    },
    [toast, settings.openAfterExtract, openArchive],
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
            // 与窗口内拖放一致：单个归档 → 打开；其它（普通文件 / 文件夹 / 多个文件，
            // 例如拖到 Dock 图标或 Windows 快捷方式上）→ 进入压缩。
            if (a.paths.length === 1 && isArchive(a.paths[0])) {
              openArchive(a.paths[0]);
            } else if (a.paths.length > 0) {
              setCreateFor(a.paths);
            }
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
  }, [openArchive, quickCreate, quickExtract]);

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
      filters: [{ name: "压缩文件", extensions: ARCHIVE_EXTS }],
    });
    if (typeof sel === "string") openArchive(sel);
  }, [openArchive]);

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
      setJob({ jobId, title: "正在解压…", progress: null });
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
        toast("ok", "解压完成");
        if (settings.openAfterExtract) openPath(out).catch(() => {});
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") toast("info", "已取消");
        else if (msg === "PASSWORD_REQUIRED") {
          const pw = await askPassword();
          if (pw) {
            setPassword(pw);
            setJob(null);
            // retry with new password
            const retryId = newJobId();
            setJob({ jobId: retryId, title: "正在解压…", progress: null });
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
              toast("ok", "解压完成");
              if (settings.openAfterExtract) openPath(out).catch(() => {});
            } catch (e2) {
              toast("error", `解压失败：${e2}`);
            }
          }
        } else toast("error", `解压失败：${msg}`);
      } finally {
        setJob(null);
      }
    },
    [archivePath, password, encoding, toast, askPassword, settings.openAfterExtract],
  );

  const runCreate = useCallback(
    async (p: { dest: string; format: string; level: number; method?: string; password?: string; volumeSize?: number }) => {
      const sources = createFor!;
      const jobId = newJobId();
      setCreateFor(null);
      setJob({ jobId, title: "正在压缩…", progress: null });
      try {
        await api.createArchive({ jobId, sources, excludeJunk: settings.excludeJunk, ...p });
        toast("ok", "压缩完成");
        // 文件系统视图下刷新当前目录，让新建的压缩包立即出现。
        if (fsListing) browseDir(fsListing.path);
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") toast("info", "已取消");
        else toast("error", `压缩失败：${msg}`);
      } finally {
        setJob(null);
      }
    },
    [createFor, toast, settings.excludeJunk, fsListing, browseDir],
  );

  const runAdd = useCallback(
    async (dir: string) => {
      if (!archivePath) return;
      const sel = await openDialog({ multiple: true, title: "选择要添加的文件" });
      const sources = Array.isArray(sel) ? sel : typeof sel === "string" ? [sel] : [];
      if (sources.length === 0) return;
      const jobId = newJobId();
      setJob({ jobId, title: "正在添加文件…", progress: null });
      try {
        await api.archiveAdd({ jobId, path: archivePath, sources, dir, password, encoding });
        toast("ok", `已添加 ${sources.length} 项`);
        await openArchive(archivePath, password, encoding);
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") toast("info", "已取消");
        else toast("error", `添加失败：${msg}`);
      } finally {
        setJob(null);
      }
    },
    [archivePath, password, encoding, openArchive, toast],
  );

  const runRemove = useCallback(
    async (entries: string[]) => {
      if (!archivePath || entries.length === 0) return;
      const jobId = newJobId();
      setJob({ jobId, title: "正在删除条目…", progress: null });
      try {
        await api.archiveRemove({ jobId, path: archivePath, entries, password, encoding });
        toast("ok", `已删除 ${entries.length} 项`);
        await openArchive(archivePath, password, encoding);
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") toast("info", "已取消");
        else toast("error", `删除失败：${msg}`);
      } finally {
        setJob(null);
      }
    },
    [archivePath, password, encoding, openArchive, toast],
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
              toast("error", `打开失败：${e2}`);
            }
          }
        } else toast("error", `打开失败：${msg}`);
      }
    },
    [archivePath, password, encoding, askPassword, toast],
  );

  const runTest = useCallback(async () => {
    if (!archivePath) return;
    const jobId = newJobId();
    setJob({ jobId, title: "正在校验…", progress: null });
    try {
      await api.testArchive(jobId, archivePath, password);
      toast("ok", "校验通过，归档完好 ✓");
    } catch (e) {
      const msg = String(e);
      if (msg === "CANCELLED") toast("info", "已取消");
      else toast("error", `校验失败：${msg}`);
    } finally {
      setJob(null);
    }
  }, [archivePath, password, toast]);

  const cancelJob = useCallback(() => {
    if (job) api.cancelJob(job.jobId);
  }, [job]);

  const title = useMemo(() => {
    if (archivePath) {
      const name = archivePath.split(/[\\/]/).pop();
      return info ? `${name} — ${info.format}` : name ?? "";
    }
    if (fsListing) return fsListing.path || "此电脑";
    return "Origami";
  }, [archivePath, info, fsListing]);

  return (
    <div className="app">
      <div className={`titlebar${isWin ? " win" : ""}`} data-tauri-drag-region>
        <span className="title" data-tauri-drag-region>{title}</span>
        <span style={{ flex: 1 }} data-tauri-drag-region />
        {archivePath && (
          <button className="btn ghost sm" onClick={closeArchive}>
            ✕ 关闭归档
          </button>
        )}
        {!archivePath && fsListing && (
          <button className="btn ghost sm" onClick={() => setFsListing(null)} title="返回主页">
            🏠 主页
          </button>
        )}
        <button className="btn ghost sm" onClick={() => setShowFinder(true)} title="右键菜单集成">
          🧩
        </button>
        <button className="btn ghost sm" onClick={() => setShowAssoc(true)} title="文件关联">
          🔗
        </button>
        <button className="btn ghost sm" onClick={() => setShowPwMgr(true)} title="密码管理器">
          🔑
        </button>
        <button className="btn ghost sm" onClick={() => setShowSettings(true)} title={isMac ? "设置 (⌘,)" : "设置 (Ctrl+,)"}>
          <span style={{ fontSize: "1.2em", lineHeight: 1 }}>⚙️</span>
        </button>
        {isWin && (
          <button className="btn ghost sm win-close" onClick={() => getCurrentWindow().close()} title="关闭">
            ✕
          </button>
        )}
      </div>

      {info && archivePath ? (
        <Browser
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
          onNavigateFs={navigateFs}
        />
      ) : fsListing ? (
        <FileExplorer
          listing={fsListing}
          loading={fsLoading}
          onNavigate={(path) => browseDir(path)}
          onOpenArchive={openArchive}
          onOpenFile={(path) => openPath(path).catch((e) => toast("error", `打开失败：${e}`))}
          onCompress={(paths) => setCreateFor(paths)}
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
