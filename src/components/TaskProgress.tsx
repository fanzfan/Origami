import { useTranslation } from "react-i18next";
import { fmtSize, type Progress } from "../api";
import { UiIcon } from "../icons";

// 快捷任务与详细设置后的任务共用一张进度卡片，窗口外观和状态保持一致。
export function TaskProgress(p: {
  verb: "compress" | "extract";
  quick?: boolean;
  progress: Progress | null;
  done?: boolean;
  error?: string | null;
  onCancel: () => void;
  onClose?: () => void;
}) {
  const { t } = useTranslation();
  const pct = p.progress && p.progress.total > 0
    ? Math.min(100, Math.max(0, (p.progress.current / p.progress.total) * 100))
    : null;
  const title = t(`task.${p.error ? "failed" : p.done ? "completed" : "running"}.${p.verb}`);

  return (
    <div className={`mini-progress task-progress ${p.error ? "has-error" : ""} ${p.done ? "is-done" : ""}`} data-tauri-drag-region>
      <div className="task-progress-head" data-tauri-drag-region>
        <span className="modal-icon" aria-hidden="true" data-tauri-drag-region>
          <UiIcon name={p.done ? "verify" : p.verb === "extract" ? "extract" : "archive"} size={20} />
        </span>
        <div className="task-progress-heading" data-tauri-drag-region>
          <div className="modal-eyebrow" data-tauri-drag-region>{t(p.quick ? "task.quick" : "task.execute")}</div>
          <div className="mini-title" role="status" data-tauri-drag-region>{title}</div>
        </div>
        <strong className="task-progress-percent" data-tauri-drag-region>
          {p.error ? "!" : p.done ? "100%" : pct === null ? "—" : `${pct.toFixed(0)}%`}
        </strong>
      </div>
      <div
        className={`progressbar ${pct === null && !p.done && !p.error ? "indeterminate" : ""}`}
        role="progressbar"
        aria-label={title}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={p.done ? 100 : pct ?? undefined}
        data-tauri-drag-region
      >
        <div style={{ width: `${p.done ? 100 : pct ?? 30}%` }} data-tauri-drag-region />
      </div>
      <div className="mini-row task-progress-bottom" data-tauri-drag-region>
        <div className={`task-progress-detail ${p.error ? "error-text" : ""}`} data-tauri-drag-region>
          {p.error ? (
            <span role="alert">{p.error}</span>
          ) : p.done ? (
            <span data-tauri-drag-region>{t("task.finished")}</span>
          ) : p.progress ? (
            <>
              <span className="task-progress-size" data-tauri-drag-region>
                {p.progress.total > 0
                  ? `${fmtSize(p.progress.current)} / ${fmtSize(p.progress.total)}`
                  : fmtSize(p.progress.current)}
              </span>
              <span className="progress-file" title={p.progress.file} data-tauri-drag-region>{p.progress.file}</span>
            </>
          ) : <span data-tauri-drag-region>{t("common.preparing")}</span>}
        </div>
        {p.error && p.onClose ? (
          <button className="btn sm" onClick={p.onClose}>{t("common.close")}</button>
        ) : !p.done && !p.error ? (
          <button className="btn sm" onClick={p.onCancel}>{t("common.cancel")}</button>
        ) : null}
      </div>
    </div>
  );
}
