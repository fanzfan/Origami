import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { api, fmtSize, newJobId, PendingAction, Progress } from "../api";
import { loadSettings } from "../settings";
import { CreateDialog, ExtractDialog } from "./dialogs";

// ask 小窗：处理需要交互的「压缩（详细设置…）」与「解压到…（选择位置）」。
// 只渲染对应对话框；确认后就地跑任务并显示进度，队列处理完即关闭小窗（见 finishAskWindow）。
// 与主窗共用同一套 CreateDialog / ExtractDialog 组件与后端命令。
export function AskDialog() {
  const [queue, setQueue] = useState<PendingAction[]>([]);
  const [idx, setIdx] = useState(0);
  const [busy, setBusy] = useState<{ verb: string } | null>(null);
  const [prog, setProg] = useState<Progress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const jobRef = useRef<string | null>(null);
  const started = useRef(false);

  const isAsk = (a: PendingAction) =>
    (a.kind === "create" && a.format === "ask") || (a.kind === "extract" && a.mode === "ask");

  // 窗口贴合卡片：按对话框类型给确定尺寸（压缩表单较高、解压较矮），再乘界面缩放 s。
  // 卡片铺满窗口、footer 固定可见；标题栏交通灯由卡片 header 顶部内边距让位（见 styles.css）。
  // 后端 spawn_ask_window 已按队首类型（scale=1）预设窗口尺寸，故默认缩放下这里应是 no-op：
  // 仅当实际尺寸与目标不符（非默认缩放，或队列切到另一类对话框）才 setSize+center，避免弹出闪烁。
  const currentKind = queue[idx]?.kind;
  useLayoutEffect(() => {
    if (!currentKind) return;
    const s = loadSettings().scale;
    const [w, h] = currentKind === "extract" ? [500, 268] : [500, 500];
    const tw = Math.round(w * s);
    const th = Math.round(h * s);
    const win = getCurrentWindow();
    (async () => {
      try {
        const factor = await win.scaleFactor();
        const cur = await win.innerSize(); // 物理像素，换算回逻辑像素与目标比较
        const cw = Math.round(cur.width / factor);
        const ch = Math.round(cur.height / factor);
        if (Math.abs(cw - tw) <= 1 && Math.abs(ch - th) <= 1) return; // 已是目标尺寸，跳过
        await win.setSize(new LogicalSize(tw, th));
        await win.center();
      } catch {
        // 兜底：拿不到当前尺寸时直接精调一次。
        await win.setSize(new LogicalSize(tw, th));
        await win.center();
      }
    })();
  }, [currentKind]);

  // 进度事件 → 更新进度条。
  useEffect(() => {
    const un = api.onProgress((p) => {
      jobRef.current = p.jobId;
      setProg(p);
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // 挂载即取走待处理的交互动作；为空则直接收尾关窗。
  useEffect(() => {
    if (started.current) return; // StrictMode 会挂载两次，避免重复取（takeAskActions 是破坏性取走）
    started.current = true;
    (async () => {
      const asks = (await api.takeAskActions()).filter(isAsk);
      if (asks.length === 0) {
        api.finishAskWindow();
        return;
      }
      setQueue(asks);
    })();
  }, []);

  // 窗口开着时又来了新的交互动作：追加到队列。
  useEffect(() => {
    const un = api.onDeepLink(async () => {
      const more = (await api.takeAskActions()).filter(isAsk);
      if (more.length > 0) setQueue((q) => [...q, ...more]);
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  // 处理完当前项：清理状态，推进到下一项；没有更多则关窗收尾。
  const next = useCallback(() => {
    setProg(null);
    setBusy(null);
    setError(null);
    const ni = idx + 1;
    if (ni >= queue.length) api.finishAskWindow();
    else setIdx(ni);
  }, [idx, queue.length]);

  const runCreate = useCallback(
    async (
      sources: string[],
      p: { dest: string; format: string; level: number; method?: string; password?: string; volumeSize?: number },
    ) => {
      setBusy({ verb: "压缩" });
      const jobId = newJobId();
      jobRef.current = jobId;
      try {
        await api.createArchive({ jobId, sources, excludeJunk: loadSettings().excludeJunk, ...p });
        next();
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") next();
        else setError(`压缩失败：${msg}`);
      }
    },
    [next],
  );

  const runExtract = useCallback(
    async (path: string, dest: string, smart: boolean) => {
      setBusy({ verb: "解压" });
      const jobId = newJobId();
      jobRef.current = jobId;
      try {
        const out = await api.extractArchive({ jobId, path, dest, smart });
        if (loadSettings().openAfterExtract) openPath(out).catch(() => {});
        next();
      } catch (e) {
        const msg = String(e);
        if (msg === "CANCELLED") next();
        else if (msg === "PASSWORD_REQUIRED") {
          // 加密归档：交给主窗打开，让用户输入密码后在应用内解压。
          await api.requestOpenInMain([path]);
          next();
        } else setError(`解压失败：${msg}`);
      }
    },
    [next],
  );

  // 跑任务时显示进度（复用迷你窗样式）。
  if (busy) {
    const pct = prog && prog.total > 0 ? Math.min(100, (prog.current / prog.total) * 100) : null;
    return (
      <div className="mini-progress" data-tauri-drag-region>
        <div className="mini-title">{error ? `${busy.verb}失败` : `正在${busy.verb}…`}</div>
        <div className={`progressbar ${pct === null && !error ? "indeterminate" : ""}`}>
          <div style={{ width: `${pct ?? 30}%` }} />
        </div>
        <div className="mini-row">
          <span className="progress-file">
            {error
              ? error
              : prog
                ? prog.total > 0
                  ? `${pct!.toFixed(0)}% · ${fmtSize(prog.current)} / ${fmtSize(prog.total)} · ${prog.file}`
                  : `${fmtSize(prog.current)} · ${prog.file}`
                : "准备中…"}
          </span>
          {error ? (
            <button className="btn sm" onClick={next}>关闭</button>
          ) : (
            <button className="btn sm" onClick={() => jobRef.current && api.cancelJob(jobRef.current)}>
              取消
            </button>
          )}
        </div>
      </div>
    );
  }

  const current = queue[idx] ?? null;
  if (!current) return null;

  if (current.kind === "create") {
    const sources = current.paths;
    return (
      <CreateDialog
        sources={sources}
        defaultLevel={loadSettings().level}
        onCancel={next}
        onConfirm={(opts) => runCreate(sources, opts)}
        pickDest={async (defName, ext) => {
          const sel = await saveDialog({
            defaultPath: defName + "." + ext,
            filters: [{ name: ext.toUpperCase(), extensions: [ext.split(".").pop()!] }],
          });
          return sel ?? null;
        }}
      />
    );
  }

  // 解压到…（选择位置）：不列归档内容，直接解压全部到所选目录。
  const path = current.paths[0];
  return (
    <ExtractDialog
      archivePath={path}
      entryCount={0}
      onCancel={next}
      onConfirm={(dest, smart) => runExtract(path, dest, smart)}
      pickDir={async () => {
        const sel = await openDialog({ directory: true, multiple: false });
        return typeof sel === "string" ? sel : null;
      }}
      defaultDir={() => api.defaultExtractDir(path)}
    />
  );
}
