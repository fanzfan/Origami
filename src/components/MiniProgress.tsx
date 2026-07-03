import { useEffect, useRef, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { api, fmtSize, newJobId, Progress } from "../api";
import { loadSettings } from "../settings";

// 迷你进度窗口：快捷压缩（资源管理器右键 / 关联启动）的「可见」驱动方。
//
// 为何由迷你窗驱动：迷你窗是可见窗口，其 WebView2 必然初始化并运行 JS；而隐藏的
// 主窗（visible:false）在部分平台（Windows WebView2）可能延迟创建 webview、不跑 JS，
// 依赖它发起任务并不可靠。故把「取动作 → 压缩 → 收尾」整条链路放在这里，并显示进度。
export function MiniProgress() {
  const [prog, setProg] = useState<Progress | null>(null);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // 当前批次动词："压缩" / "解压"，用于标题与提示文案。
  const [verb, setVerb] = useState("压缩");
  const jobRef = useRef<string | null>(null);
  const started = useRef(false);

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

  // 挂载即驱动：取走积压的快捷压缩动作，逐个压缩，最后收尾。
  useEffect(() => {
    if (started.current) return; // React StrictMode 会挂载两次，避免重复发起
    started.current = true;
    (async () => {
      const settings = loadSettings();
      const actions = await api.takePendingActions();
      const creates = actions.filter(
        (a): a is { kind: "create"; format: string; paths: string[] } =>
          a.kind === "create" && a.format !== "ask",
      );
      const extracts = actions.filter(
        (a): a is { kind: "extract"; mode: string; paths: string[] } =>
          a.kind === "extract" && a.mode !== "ask",
      );
      if (creates.length === 0 && extracts.length === 0) {
        await api.endQuickJob(true);
        return;
      }
      // 批次通常只含一种动作（右键菜单一次一项）；有解压则标题显示「解压」。
      const label = extracts.length > 0 && creates.length === 0 ? "解压" : "压缩";
      setVerb(label);

      let ok = true;
      let lastErr = "";
      // 遇到需要密码等交互的归档，转交主窗打开。
      const needMain: string[] = [];

      for (const a of creates) {
        const jobId = newJobId();
        jobRef.current = jobId;
        try {
          const dest = await api.defaultCreateDest(a.paths, a.format);
          await api.createArchive({
            jobId,
            dest,
            sources: a.paths,
            format: a.format,
            level: settings.level,
            excludeJunk: settings.excludeJunk,
          });
        } catch (e) {
          const msg = String(e);
          if (msg !== "CANCELLED") {
            ok = false;
            lastErr = msg;
          }
        }
      }

      for (const a of extracts) {
        for (const path of a.paths) {
          const jobId = newJobId();
          jobRef.current = jobId;
          try {
            const dest = await api.quickExtractDest(path, a.mode);
            const out = await api.extractArchive({ jobId, path, dest, smart: false });
            if (settings.openAfterExtract) openPath(out).catch(() => {});
          } catch (e) {
            const msg = String(e);
            if (msg === "CANCELLED") {
              // 用户主动取消，不算失败。
            } else if (msg === "PASSWORD_REQUIRED") {
              needMain.push(path); // 需要输入密码，交给主窗
            } else {
              ok = false;
              lastErr = msg;
            }
          }
        }
      }

      // 有归档需要密码：显示主窗并打开它，让用户在应用内输入密码后解压。
      if (needMain.length > 0) {
        await api.requestOpenInMain(needMain);
        await api.endQuickJob(true); // 存在待处理动作，endQuickJob 不会退出，主窗保留
        return;
      }

      if (ok) {
        setDone(true);
        await new Promise((r) => setTimeout(r, 800));
      } else {
        setError(lastErr || `${label}失败`);
        // 出错时多停留片刻让用户看到信息，随后由主窗接管报错。
        await new Promise((r) => setTimeout(r, 1800));
      }
      await api.endQuickJob(ok);
    })();
  }, []);

  const pct = prog && prog.total > 0 ? Math.min(100, (prog.current / prog.total) * 100) : null;

  return (
    <div className="mini-progress" data-tauri-drag-region>
      <div className="mini-title">
        {error ? `${verb}失败` : done ? `${verb}完成 ✓` : `正在${verb}…`}
      </div>
      <div
        className={`progressbar ${pct === null && !done && !error ? "indeterminate" : ""}`}
      >
        <div style={{ width: `${done ? 100 : pct ?? 30}%` }} />
      </div>
      <div className="mini-row">
        <span className="progress-file">
          {error
            ? error
            : done
              ? ""
              : prog
                ? prog.total > 0
                  ? `${pct!.toFixed(0)}% · ${fmtSize(prog.current)} / ${fmtSize(prog.total)} · ${prog.file}`
                  : `${fmtSize(prog.current)} · ${prog.file}`
                : "准备中…"}
        </span>
        {!done && !error && (
          <button
            className="btn sm"
            onClick={() => jobRef.current && api.cancelJob(jobRef.current)}
          >
            取消
          </button>
        )}
      </div>
    </div>
  );
}
