import { useEffect, useRef, useState } from "react";
import { api, fmtSize, Progress } from "../api";

export function MiniProgress() {
  const [prog, setProg] = useState<Progress | null>(null);
  const jobRef = useRef<string | null>(null);

  useEffect(() => {
    const un = api.onProgress((p) => {
      jobRef.current = p.jobId;
      setProg(p);
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  const pct = prog && prog.total > 0 ? Math.min(100, (prog.current / prog.total) * 100) : null;
  const done = prog?.done ?? false;

  return (
    <div className="mini-progress" data-tauri-drag-region>
      <div className="mini-title">{done ? "压缩完成 ✓" : "正在压缩…"}</div>
      <div className={`progressbar ${pct === null && !done ? "indeterminate" : ""}`}>
        <div style={{ width: `${done ? 100 : pct ?? 30}%` }} />
      </div>
      <div className="mini-row">
        <span className="progress-file">
          {done
            ? ""
            : prog
              ? prog.total > 0
                ? `${pct!.toFixed(0)}% · ${fmtSize(prog.current)} / ${fmtSize(prog.total)} · ${prog.file}`
                : `${fmtSize(prog.current)} · ${prog.file}`
              : "准备中…"}
        </span>
        {!done && (
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
