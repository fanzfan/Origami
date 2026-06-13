import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { api, ArchiveInfo, newJobId, Progress } from "./api";
import { Welcome } from "./components/Welcome";
import { Browser } from "./components/Browser";
import {
  CreateDialog,
  ExtractDialog,
  ShellIntegration,
  PasswordManager,
  PasswordPrompt,
  PreviewModal,
  ProgressModal,
  SettingsDialog,
} from "./components/dialogs";
import { applySettings, clampScale, loadSettings, saveSettings, SCALE_STEP } from "./settings";

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

const ARCHIVE_EXTS = [
  "zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "tbz2", "xz", "txz", "zst", "tzst", "jar", "apk",
];

function isArchive(path: string): boolean {
  const lower = path.toLowerCase();
  return ARCHIVE_EXTS.some((e) => lower.endsWith("." + e));
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
  const [showSettings, setShowSettings] = useState(false);
  const [settings, setSettings] = useState(loadSettings);
  const [extractFor, setExtractFor] = useState<{ entries: string[] } | null>(null);
  const [createFor, setCreateFor] = useState<string[] | null>(null);
  const [previewPath, setPreviewPath] = useState<string | null>(null);

  // ----- settings: persist + apply (zoom + font), plus Ctrl/Cmd +/-/0 -----
  useEffect(() => {
    applySettings(settings);
    saveSettings(settings);
  }, [settings]);

  const setScale = useCallback((s: number) => {
    setSettings((cur) => ({ ...cur, scale: clampScale(s) }));
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
      if (e.key === "=" || e.key === "+") {
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
  }, []);

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
        const out = await api.createArchive({ jobId, dest, sources, format, level: 6 });
        toast("ok", `压缩完成：${out.split("/").pop()}`);
        revealItemInDir(out).catch(() => {});
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
    [toast],
  );

  const draining = useRef(false);
  const drainActions = useCallback(async () => {
    if (draining.current) return; // 进行中的循环会在下一轮取走新动作
    draining.current = true;
    try {
      for (;;) {
        const actions = await api.takePendingActions();
        if (actions.length === 0) break;
        const quick: { format: string; paths: string[] }[] = [];
        for (const a of actions) {
          if (a.kind === "open") {
            const arc = a.paths.find(isArchive);
            if (arc) openArchive(arc);
          } else if (a.format === "ask") {
            setCreateFor(a.paths);
          } else {
            quick.push(a);
          }
        }
        if (quick.length > 0) {
          const useMini = await api.beginQuickJob();
          let ok = true;
          for (const a of quick) {
            ok = (await quickCreate(a.format, a.paths, useMini)) && ok;
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
  }, [openArchive, quickCreate]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      unlisten = await api.onDeepLink(drainActions);
      await drainActions();
      await api.frontendReady();
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
        revealItemInDir(out).catch(() => {});
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
              revealItemInDir(out).catch(() => {});
            } catch (e2) {
              toast("error", `解压失败：${e2}`);
            }
          }
        } else toast("error", `解压失败：${msg}`);
      } finally {
        setJob(null);
      }
    },
    [archivePath, password, encoding, toast, askPassword],
  );

  const runCreate = useCallback(
    async (p: { dest: string; format: string; level: number; method?: string; password?: string; volumeSize?: number }) => {
      const sources = createFor!;
      const jobId = newJobId();
      setCreateFor(null);
      setJob({ jobId, title: "正在压缩…", progress: null });
      try {
        const out = await api.createArchive({ jobId, sources, ...p });
        toast("ok", "压缩完成");
        revealItemInDir(out).catch(() => {});
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") toast("info", "已取消");
        else toast("error", `压缩失败：${msg}`);
      } finally {
        setJob(null);
      }
    },
    [createFor, toast],
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
    if (!archivePath) return "Origami";
    const name = archivePath.split("/").pop();
    return info ? `${name} — ${info.format}` : name ?? "";
  }, [archivePath, info]);

  return (
    <div className="app">
      <div className="titlebar">
        <span className="title">{title}</span>
        <span style={{ flex: 1 }} />
        {archivePath && (
          <button className="btn ghost sm" onClick={closeArchive}>
            ✕ 关闭归档
          </button>
        )}
        <button className="btn ghost sm" onClick={() => setShowFinder(true)} title="右键菜单集成">
          🧩
        </button>
        <button className="btn ghost sm" onClick={() => setShowPwMgr(true)} title="密码管理器">
          🔑
        </button>
        <button className="btn ghost sm" onClick={() => setShowSettings(true)} title="设置">
          ⚙
        </button>
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
          onAdd={runAdd}
          onRemove={runRemove}
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

      {showSettings && (
        <SettingsDialog
          scale={settings.scale}
          font={settings.font}
          onScale={setScale}
          onFont={(f) => setSettings((c) => ({ ...c, font: f }))}
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
