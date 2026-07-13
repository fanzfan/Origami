import { useEffect, useId, useMemo, useRef, useState, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { api, AssocEntry, fmtDate, fmtSize, Preview, PwMeta } from "../api";
import { LANGUAGE_OPTIONS, type AppLanguage } from "../i18n";
import { UiIcon, type UiIconName } from "../icons";
import type { JobState } from "../App";
import { ACRYLIC_OPACITY_MAX, ACRYLIC_OPACITY_MIN, FONTS, MATERIALS, MODES, SCALE_MAX, SCALE_MIN, SCALE_STEP, THEMES, clampScale, type Settings } from "../settings";

function Modal(p: {
  title: string;
  eyebrow?: string;
  description?: string;
  icon?: UiIconName;
  className?: string;
  wide?: boolean;
  children: React.ReactNode;
  footer?: React.ReactNode;
  onClose?: () => void;
}) {
  const titleId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!p.onClose) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.isComposing || e.defaultPrevented || e.keyCode === 229) return;
      if (e.key === "Escape") p.onClose?.();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [p.onClose]);

  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const frame = requestAnimationFrame(() => {
      const target = dialogRef.current?.querySelector<HTMLElement>(
        ".body input:not([disabled]), .body select:not([disabled]), .body button:not([disabled]), footer button:not([disabled]), .modal-close",
      );
      target?.focus();
    });
    return () => {
      cancelAnimationFrame(frame);
      if (previous?.isConnected) previous.focus();
    };
  }, []);

  const trapFocus = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "Tab") return;
    const focusable = Array.from(
      dialogRef.current?.querySelectorAll<HTMLElement>(
        "button:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex='-1'])",
      ) ?? [],
    ).filter((el) => el.offsetParent !== null);
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  };

  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && p.onClose?.()}>
      <div
        ref={dialogRef}
        className={["modal", p.wide ? "wide" : "", p.className ?? ""].filter(Boolean).join(" ")}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onKeyDown={trapFocus}
      >
        <header className="modal-header" data-tauri-drag-region>
          <div className="modal-heading" data-tauri-drag-region>
            {p.icon && (
              <span className="modal-icon" aria-hidden="true" data-tauri-drag-region>
                <UiIcon name={p.icon} size={20} />
              </span>
            )}
            <div className="modal-heading-copy" data-tauri-drag-region>
              {p.eyebrow && <div className="modal-eyebrow">{p.eyebrow}</div>}
              <h2 id={titleId}>{p.title}</h2>
              {p.description && <p>{p.description}</p>}
            </div>
          </div>
          {p.onClose && (
            <button className="btn ghost modal-close" onClick={p.onClose} aria-label="关闭" title="关闭">
              <UiIcon name="close" size={17} />
            </button>
          )}
        </header>
        <div className="body">{p.children}</div>
        {p.footer && <footer>{p.footer}</footer>}
      </div>
    </div>
  );
}

// ---------------- Extract ----------------

export function ExtractDialog(p: {
  archivePath: string;
  entryCount: number;
  onCancel: () => void;
  onConfirm: (dest: string, smart: boolean) => void;
  pickDir: () => Promise<string | null>;
  defaultDir: () => Promise<string>;
}) {
  const [dest, setDest] = useState("");
  const [smart, setSmart] = useState(true);
  const archiveName = p.archivePath.split(/[\\/]/).pop() || p.archivePath;

  useEffect(() => {
    p.defaultDir().then(setDest);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <Modal
      title={p.entryCount > 0 ? `解压选中的 ${p.entryCount} 项` : "解压全部"}
      eyebrow="解压归档"
      description="选择目标位置，Origami 会保持目录结构清晰。"
      icon="extract"
      className="task-dialog extract-dialog"
      onClose={p.onCancel}
      footer={
        <>
          <button className="btn" onClick={p.onCancel}>取消</button>
          <button className="btn primary" disabled={!dest} onClick={() => p.onConfirm(dest, smart)}>
            <UiIcon name="extract" size={15} />
            开始解压
          </button>
        </>
      }
    >
      <div className="task-source" title={p.archivePath}>
        <span className="task-source-icon"><UiIcon name="file-archive" size={19} /></span>
        <span className="task-source-main">
          <span className="task-source-label">来源归档</span>
          <strong>{archiveName}</strong>
          <span className="task-source-path">{p.archivePath}</span>
        </span>
      </div>
      <div className="field">
        <label>解压到</label>
        <div className="row path-picker">
          <input type="text" value={dest} onChange={(e) => setDest(e.target.value)} />
          <button
            className="btn"
            onClick={async () => {
              const d = await p.pickDir();
              if (d) setDest(d);
            }}
          >
            <UiIcon name="folder-open" size={15} />
            浏览…
          </button>
        </div>
      </div>
      <label className="smart-option">
        <input type="checkbox" checked={smart} onChange={(e) => setSmart(e.target.checked)} />
        <span className="smart-option-copy">
          <strong>智能解压</strong>
          <span>多个顶层文件时自动创建文件夹，避免文件散落。</span>
        </span>
      </label>
    </Modal>
  );
}

// ---------------- Create ----------------

const FORMATS = [
  { v: "zip", label: "ZIP", enc: true },
  { v: "7z", label: "7Z", enc: true },
  { v: "tar.gz", label: "TAR.GZ", enc: false },
  { v: "tar.xz", label: "TAR.XZ", enc: false },
  { v: "tar.bz2", label: "TAR.BZ2", enc: false },
  { v: "tar.zst", label: "TAR.ZST", enc: false },
  { v: "tar", label: "TAR", enc: false },
];

const METHODS: Record<string, [string, string][]> = {
  zip: [
    ["", "Deflate（默认，兼容性最好）"],
    ["bzip2", "BZip2"],
    ["zstd", "Zstandard（快）"],
    ["xz", "XZ/LZMA（高压缩率）"],
    ["ppmd", "PPMd（文本最优）"],
  ],
  "7z": [
    ["", "LZMA2（默认）"],
    ["bzip2", "BZip2"],
    ["zstd", "Zstandard（快）"],
    ["ppmd", "PPMd（文本最优）"],
    ["copy", "仅存储"],
  ],
};

const VOLUMES: [number, string][] = [
  [0, "不分卷"],
  [10 * 1024 * 1024, "10 MB"],
  [100 * 1024 * 1024, "100 MB"],
  [700 * 1024 * 1024, "700 MB"],
  [1024 * 1024 * 1024, "1 GB"],
  [4 * 1024 * 1024 * 1024 - 1, "FAT32 (4GB-1)"],
];

export function CreateDialog(p: {
  sources: string[];
  defaultLevel?: number;
  onCancel: () => void;
  onConfirm: (opts: { dest: string; format: string; level: number; method?: string; password?: string; volumeSize?: number }) => void;
  pickDest: (defName: string, ext: string) => Promise<string | null>;
}) {
  const defName = (p.sources[0]?.split("/").pop() ?? "archive").replace(/\.[^.]+$/, "") || "archive";
  const [format, setFormat] = useState("zip");
  const [method, setMethod] = useState("");
  const [level, setLevel] = useState(p.defaultLevel ?? 6);
  const [password, setPassword] = useState("");
  const [volume, setVolume] = useState(0);
  const [dest, setDest] = useState("");

  const fmt = FORMATS.find((f) => f.v === format)!;
  const sourceName = p.sources[0]?.split(/[\\/]/).pop() || p.sources[0] || "未命名项目";
  const levelLabel = level === 0 ? "仅存储" : level <= 3 ? "更快" : level <= 6 ? "均衡" : level < 9 ? "更小" : "最高";

  return (
    <Modal
      title={`压缩 ${p.sources.length} 项`}
      eyebrow="创建归档"
      description="设置输出位置、格式与压缩参数。"
      icon="archive"
      className="task-dialog create-dialog"
      onClose={p.onCancel}
      footer={
        <>
          <button className="btn" onClick={p.onCancel}>取消</button>
          <button
            className="btn primary"
            disabled={!dest}
            onClick={() =>
              p.onConfirm({
                dest,
                format,
                level,
                method: method || undefined,
                password: fmt.enc && password ? password : undefined,
                volumeSize: volume || undefined,
              })
            }
          >
            <UiIcon name="archive" size={15} />
            开始压缩
          </button>
        </>
      }
    >
      <div className="task-source" title={p.sources.join("\n")}>
        <span className="task-source-icon">
          <UiIcon name={p.sources.length > 1 ? "folder-archive" : "file-archive"} size={19} />
        </span>
        <span className="task-source-main">
          <span className="task-source-label">待压缩{p.sources.length > 1 ? ` · ${p.sources.length} 项` : ""}</span>
          <strong>{sourceName}</strong>
          <span className="task-source-path">
            {p.sources[0]}{p.sources.length > 1 ? ` · 另有 ${p.sources.length - 1} 项` : ""}
          </span>
        </span>
      </div>

      <div className="field">
        <label>保存为</label>
        <div className="row path-picker">
          <input type="text" value={dest} onChange={(e) => setDest(e.target.value)} placeholder="选择保存位置…" />
          <button
            className="btn"
            onClick={async () => {
              const d = await p.pickDest(defName, format);
              if (d) setDest(d);
            }}
          >
            <UiIcon name="folder-open" size={15} />
            浏览…
          </button>
        </div>
      </div>

      <div className="row form-grid">
        <div className="field">
          <label>格式</label>
          <select
            value={format}
            onChange={(e) => {
              const next = e.target.value;
              setFormat(next);
              setMethod("");
              setDest((d) => (d ? d.replace(/\.(zip|7z|tar(\.(gz|xz|bz2|zst))?)$/i, "") + "." + next : d));
            }}
          >
            {FORMATS.map((f) => (
              <option key={f.v} value={f.v}>
                {f.label}
              </option>
            ))}
          </select>
        </div>
        <div className="field">
          <label>分卷</label>
          <select value={volume} onChange={(e) => setVolume(Number(e.target.value))}>
            {VOLUMES.map(([v, l]) => (
              <option key={v} value={v}>
                {l}
              </option>
            ))}
          </select>
        </div>
      </div>

      {METHODS[format] && (
        <div className="field">
          <label>算法</label>
          <select value={method} onChange={(e) => setMethod(e.target.value)}>
            {METHODS[format].map(([v, l]) => (
              <option key={v} value={v}>
                {l}
              </option>
            ))}
          </select>
        </div>
      )}

      <div className="field level-field">
        <div className="field-label-row">
          <label>压缩等级</label>
          <span className="value-pill">{level === 0 ? "0" : level} · {levelLabel}</span>
        </div>
        <input
          className="level-range"
          type="range"
          min={0}
          max={9}
          value={level}
          aria-label="压缩等级"
          style={{ "--range-progress": `${(level / 9) * 100}%` } as CSSProperties}
          onChange={(e) => setLevel(Number(e.target.value))}
        />
        <div className="range-labels"><span>更快</span><span>更小</span></div>
      </div>

      {fmt.enc && (
        <div className="field">
          <label>密码（可选，{format === "zip" ? "AES-256" : "AES-256 + 加密文件名"}）</label>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="留空表示不加密"
          />
        </div>
      )}
      {volume > 0 && <div className="task-note">分卷输出为 .001 / .002 … 文件；解压时请选择 .001。</div>}
    </Modal>
  );
}

// ---------------- Settings ----------------

export function SettingsDialog(p: {
  settings: Settings;
  onChange: (patch: Partial<Settings>) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const s = p.settings;
  const pct = Math.round(s.scale * 100);
  const setScale = (v: number) => p.onChange({ scale: clampScale(v) });
  const isWin = navigator.userAgent.includes("Windows") || navigator.platform.startsWith("Win");
  const levelValue = s.level === 0
    ? t("settings.level.store")
    : `${s.level}${s.level === 9 ? t("settings.level.highest") : ""}`;
  const levelHint = s.level === 0
    ? t("settings.level.storeHint")
    : s.level === 9
      ? t("settings.level.highestHint")
      : t("settings.level.defaultHint");

  return (
    <Modal
      title={t("settings.title")}
      onClose={p.onClose}
      footer={<button className="btn primary" onClick={p.onClose}>{t("common.done")}</button>}
    >
      <div className="settings-section">{t("settings.section.general")}</div>

      <div className="field">
        <label>{t("settings.language.label")}</label>
        <select
          value={s.language}
          onChange={(e) => p.onChange({ language: e.target.value as AppLanguage })}
        >
          {LANGUAGE_OPTIONS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.nativeName ?? t("settings.language.system")}
            </option>
          ))}
        </select>
        <div className="hint">{t("settings.language.hint")}</div>
      </div>

      <div className="settings-section">{t("settings.section.appearance")}</div>

      <div className="field">
        <label>{t("settings.appearanceMode")}</label>
        <div className="seg">
          {MODES.map((key) => (
            <button
              key={key}
              className={`seg-btn ${s.mode === key ? "on" : ""}`}
              onClick={() => p.onChange({ mode: key })}
            >
              {t(`settings.mode.${key}`)}
            </button>
          ))}
        </div>
      </div>

      <div className="field">
        <label>{t("settings.themeLabel")}</label>
        <div className="theme-grid">
          {THEMES.map(([key, preview]) => (
            <div
              key={key}
              className={`theme-swatch ${s.theme === key ? "on" : ""}`}
              onClick={() => p.onChange({ theme: key })}
            >
              <span className="dot" style={{ background: preview }} />
              {t(`settings.theme.${key}`)}
            </div>
          ))}
        </div>
      </div>

      <div className="field">
        <label>{t("settings.scale.label", { percent: pct })}</label>
        <div className="row" style={{ alignItems: "center", gap: 10 }}>
          <button className="btn sm" onClick={() => setScale(s.scale - SCALE_STEP)}>－</button>
          <input
            type="range"
            min={SCALE_MIN}
            max={SCALE_MAX}
            step={SCALE_STEP}
            value={s.scale}
            onChange={(e) => setScale(Number(e.target.value))}
            style={{ flex: 1 }}
          />
          <button className="btn sm" onClick={() => setScale(s.scale + SCALE_STEP)}>＋</button>
          <button className="btn sm" onClick={() => setScale(1)}>{t("settings.scale.reset")}</button>
        </div>
        <div className="hint">{t("settings.scale.hint")}</div>
      </div>

      <div className="field">
        <label>{t("settings.fontLabel")}</label>
        <select value={s.font} onChange={(e) => p.onChange({ font: e.target.value })}>
          {FONTS.map(([key]) => (
            <option key={key} value={key}>
              {t(`settings.font.${key}`)}
            </option>
          ))}
        </select>
      </div>

      {isWin ? (
        <div className="field">
          <label>{t("settings.material.label")}</label>
          <select value={s.material} onChange={(e) => p.onChange({ material: e.target.value as Settings["material"] })}>
            {MATERIALS.map((key) => (
              <option key={key} value={key}>
                {t(`settings.material.${key}`)}
              </option>
            ))}
          </select>
          <div className="hint">{t("settings.material.windowsHint")}</div>
          {s.material === "acrylic" && (
            <div style={{ marginTop: 10 }}>
              <label>{t("settings.material.opacity", { percent: 100 - s.acrylicOpacity })}</label>
              <input
                type="range"
                min={100 - ACRYLIC_OPACITY_MAX}
                max={100 - ACRYLIC_OPACITY_MIN}
                value={100 - s.acrylicOpacity}
                onChange={(e) => p.onChange({ acrylicOpacity: 100 - Number(e.target.value) })}
                style={{ width: "100%" }}
              />
              <div className="hint">{t("settings.material.opacityHint")}</div>
            </div>
          )}
        </div>
      ) : (
        <label className="toggle-row">
          <input
            type="checkbox"
            checked={s.material !== "none"}
            onChange={(e) => p.onChange({ material: e.target.checked ? "acrylic" : "none" })}
          />
          <span className="grow">
            {t("settings.material.effect")}
            <div className="hint">{t("settings.material.macHint")}</div>
          </span>
        </label>
      )}

      <div className="settings-section">{t("settings.section.archive")}</div>

      <div className="field">
        <label>{t("settings.level.label", { value: levelValue })}</label>
        <input
          type="range"
          min={0}
          max={9}
          value={s.level}
          onChange={(e) => p.onChange({ level: Number(e.target.value) })}
        />
        <div className="hint">{levelHint}</div>
      </div>

      <label className="toggle-row">
        <input
          type="checkbox"
          checked={s.excludeJunk}
          onChange={(e) => p.onChange({ excludeJunk: e.target.checked })}
        />
        <span className="grow">
          {t("settings.excludeJunk.label")}
          <div className="hint">{t("settings.excludeJunk.hint")}</div>
        </span>
      </label>

      <label className="toggle-row">
        <input
          type="checkbox"
          checked={s.openAfterExtract}
          onChange={(e) => p.onChange({ openAfterExtract: e.target.checked })}
        />
        <span className="grow">
          {t("settings.openAfterExtract.label")}
          <div className="hint">{t("settings.openAfterExtract.hint")}</div>
        </span>
      </label>
    </Modal>
  );
}

// ---------------- Password prompt ----------------

export function PasswordPrompt(p: { onSubmit: (pw: string | null) => void }) {
  const [pw, setPw] = useState("");
  const [save, setSave] = useState(true);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => ref.current?.focus(), []);

  const submit = () => {
    if (!pw) return;
    if (save) api.pwAdd(pw).catch(() => {});
    p.onSubmit(pw);
  };

  return (
    <Modal
      title="需要密码"
      onClose={() => p.onSubmit(null)}
      footer={
        <>
          <button className="btn" onClick={() => p.onSubmit(null)}>取消</button>
          <button className="btn primary" disabled={!pw} onClick={submit}>确定</button>
        </>
      }
    >
      <p className="hint" style={{ margin: 0 }}>
        此归档已加密。已尝试密码管理器中保存的密码，均不匹配。
      </p>
      <input
        ref={ref}
        type="password"
        value={pw}
        placeholder="输入密码"
        onChange={(e) => setPw(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && submit()}
      />
      <label className="row" style={{ gap: 8 }}>
        <input type="checkbox" checked={save} onChange={(e) => setSave(e.target.checked)} style={{ width: "auto" }} />
        <span>保存到密码管理器，下次自动尝试</span>
      </label>
    </Modal>
  );
}

// ---------------- Password manager ----------------

type AuthPhase = "checking" | "ready" | "denied";

export function PasswordManager(p: { onClose: () => void }) {
  const [list, setList] = useState<PwMeta[]>([]);
  // 明文按需加载：id -> 明文。仅当用户勾选「显示密码」时才向后端索取（届时才读凭据库）。
  const [secrets, setSecrets] = useState<Record<string, string>>({});
  const [newPw, setNewPw] = useState("");
  const [newLabel, setNewLabel] = useState("");
  const [reveal, setReveal] = useState(false);
  const [phase, setPhase] = useState<AuthPhase>("checking");
  // 正在拖动的条目 id（仅用于样式）。
  const [dragId, setDragId] = useState<string | null>(null);

  // 列表只取元数据，不读凭据库（打开管理器不会弹钥匙串）。
  const refresh = () => api.pwList().then(setList);

  // 拖动排序用：实时镜像最新 list（供拖放结束时持久化）与每行 DOM（供命中测试）。
  const listRef = useRef<PwMeta[]>([]);
  listRef.current = list;
  const rowRefs = useRef<Record<string, HTMLDivElement | null>>({});
  const dragIdRef = useRef<string | null>(null);

  // 勾选「显示密码」时再加载明文；取消则清空（不缓存明文）。
  const toggleReveal = async (on: boolean) => {
    setReveal(on);
    if (on) {
      const revealed = await api.pwReveal();
      setSecrets(Object.fromEntries(revealed.map((r) => [r.id, r.password])));
    } else {
      setSecrets({});
    }
  };

  const addPassword = async () => {
    if (!newPw) return;
    await api.pwAdd(newPw, newLabel || undefined);
    setNewPw("");
    setNewLabel("");
    refresh();
    if (reveal) toggleReveal(true);
  };

  // 拖动排序：用鼠标事件实现（WKWebView 对 HTML5 拖放支持不可靠，故不依赖 draggable）。
  // 拖动时按各行垂直中点做命中测试，实时重排；松手后把新顺序持久化。
  const startDrag = (id: string, e: React.MouseEvent) => {
    e.preventDefault(); // 阻止文本选中
    dragIdRef.current = id;
    setDragId(id);

    const onMove = (ev: MouseEvent) => {
      const dragging = dragIdRef.current;
      if (!dragging) return;
      setList((cur) => {
        const from = cur.findIndex((x) => x.id === dragging);
        if (from < 0) return cur;
        let to = cur.length - 1;
        for (let i = 0; i < cur.length; i++) {
          const el = rowRefs.current[cur[i].id];
          if (!el) continue;
          const r = el.getBoundingClientRect();
          if (ev.clientY < r.top + r.height / 2) {
            to = i;
            break;
          }
        }
        if (to === from) return cur;
        const next = [...cur];
        const [moved] = next.splice(from, 1);
        next.splice(to, 0, moved);
        return next;
      });
    };

    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      dragIdRef.current = null;
      setDragId(null);
      api.pwReorder(listRef.current.map((x) => x.id)).then(refresh);
    };

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  const authenticate = async () => {
    setPhase("checking");
    try {
      const need = await api.systemAuthAvailable();
      if (!need) {
        setPhase("ready");
        refresh();
        return;
      }
      const ok = await api.systemAuth("查看已保存的归档密码");
      if (ok) {
        setPhase("ready");
        refresh();
      } else {
        setPhase("denied");
      }
    } catch {
      // 认证机制异常时不锁死用户查看自己的密码。
      setPhase("ready");
      refresh();
    }
  };

  useEffect(() => {
    authenticate();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (phase !== "ready") {
    return (
      <Modal
        title="密码管理器"
        onClose={p.onClose}
        footer={
          <>
            <button className="btn" onClick={p.onClose}>关闭</button>
            {phase === "denied" && (
              <button className="btn primary" onClick={authenticate}>重试验证</button>
            )}
          </>
        }
      >
        <div className="auth-gate">
          <div className="auth-icon">{phase === "denied" ? "🔒" : "🪪"}</div>
          {phase === "checking" ? (
            <p>正在通过系统验证你的身份…<br /><span className="hint">请在弹出的系统对话框中使用指纹 / 面容 / 系统密码。</span></p>
          ) : (
            <p>身份验证未通过，无法查看已保存的密码。<br /><span className="hint">出于安全考虑，需先通过系统验证。</span></p>
          )}
        </div>
      </Modal>
    );
  }

  return (
    <Modal
      title="密码管理器"
      wide
      onClose={p.onClose}
      footer={<button className="btn primary" onClick={p.onClose}>完成</button>}
    >
      <p className="hint" style={{ margin: 0 }}>
        打开或解压加密归档时，会按下面的列表顺序尝试这些密码；可拖动左侧 ⇅ 调整顺序。
      </p>
      <div className="row">
        <input
          type={reveal ? "text" : "password"}
          placeholder="新密码"
          value={newPw}
          onChange={(e) => setNewPw(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              addPassword();
            }
          }}
        />
        <input
          type="text"
          placeholder="备注（可选）"
          value={newLabel}
          onChange={(e) => setNewLabel(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              addPassword();
            }
          }}
        />
        <button className="btn" disabled={!newPw} onClick={addPassword}>
          添加
        </button>
      </div>
      <label className="row" style={{ gap: 8 }}>
        <input type="checkbox" checked={reveal} onChange={(e) => toggleReveal(e.target.checked)} style={{ width: "auto" }} />
        <span className="hint">显示密码</span>
      </label>
      <div className="pwlist">
        {list.length === 0 && <div className="hint">还没有保存的密码</div>}
        {list.map((e) => (
          <div
            className={`pw-item${dragId === e.id ? " dragging" : ""}`}
            key={e.id}
            ref={(el) => {
              rowRefs.current[e.id] = el;
            }}
          >
            <span
              className="drag-handle"
              title="拖动调整顺序"
              aria-label="拖动调整顺序"
              onMouseDown={(ev) => startDrag(e.id, ev)}
            >
              ⇅
            </span>
            <code>{reveal ? (secrets[e.id] ?? "") : "••••••••"}</code>
            {e.label && <span className="label">{e.label}</span>}
            <button
              className="btn sm danger"
              onClick={async () => {
                await api.pwRemove(e.id);
                refresh();
              }}
            >
              删除
            </button>
          </div>
        ))}
      </div>
    </Modal>
  );
}

// ---------------- File associations ----------------

export function FileAssociations(p: { onClose: () => void; toast: (kind: "ok" | "error" | "info", text: string) => void }) {
  const [list, setList] = useState<AssocEntry[] | null>(null);
  const [desired, setDesired] = useState<Record<string, boolean>>({});
  const [supported, setSupported] = useState<boolean | null>(null);
  const [os, setOs] = useState<string>("");
  const [busy, setBusy] = useState(false);

  const refresh = async () => {
    const rows = await api.fileAssocList();
    setList(rows);
    setDesired(Object.fromEntries(rows.map((r) => [r.ext, r.associated])));
  };

  useEffect(() => {
    api.appPlatform().then(setOs);
    api.fileAssocSupported().then(setSupported);
    refresh();
  }, []);

  const isWin = os === "windows";
  const baseline = useMemo(() => Object.fromEntries((list ?? []).map((r) => [r.ext, r.associated])), [list]);
  const dirty = useMemo(
    () => (list ?? []).some((r) => desired[r.ext] !== baseline[r.ext]),
    [list, desired, baseline],
  );

  const setAll = (v: boolean) => setDesired(Object.fromEntries((list ?? []).map((r) => [r.ext, v])));

  const apply = async () => {
    const toAssoc = (list ?? []).filter((r) => desired[r.ext] && !baseline[r.ext]).map((r) => r.ext);
    const toRemove = (list ?? []).filter((r) => !desired[r.ext] && baseline[r.ext]).map((r) => r.ext);
    setBusy(true);
    try {
      if (toAssoc.length) await api.fileAssocSet(toAssoc, true);
      if (toRemove.length) await api.fileAssocSet(toRemove, false);
      await refresh();
      p.toast("ok", `已更新 ${toAssoc.length + toRemove.length} 个文件关联`);
    } catch (e) {
      p.toast("error", `更新失败：${e}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title="文件关联"
      onClose={p.onClose}
      footer={
        <>
          <button className="btn" onClick={p.onClose}>关闭</button>
          <button className="btn primary" disabled={busy || !dirty || supported === false} onClick={apply}>
            应用更改
          </button>
        </>
      }
    >
      {supported === false ? (
        <p className="hint" style={{ margin: 0 }}>当前平台不支持在应用内管理文件关联。</p>
      ) : (
        <>
          <p className="hint" style={{ margin: 0 }}>
            勾选要让 Origami 作为默认打开程序的压缩格式，然后点「应用更改」。
          </p>
          <div className="row" style={{ justifyContent: "flex-start", gap: 8 }}>
            <button className="btn sm" onClick={() => setAll(true)}>全选</button>
            <button className="btn sm" onClick={() => setAll(false)}>全不选</button>
          </div>
          <div className="assoc-list">
            {!list && <div className="hint">加载中…</div>}
            {list?.map((r) => (
              <label className="assoc-item" key={r.ext}>
                <input
                  type="checkbox"
                  checked={desired[r.ext] ?? false}
                  onChange={(e) => setDesired((d) => ({ ...d, [r.ext]: e.target.checked }))}
                  style={{ width: "auto" }}
                />
                <span className="ext">.{r.ext}</span>
                <span className="cur hint">
                  {r.associated ? "当前：Origami" : r.currentApp ? `当前：${r.currentApp}` : "当前：系统默认"}
                </span>
              </label>
            ))}
          </div>
          <p className="hint" style={{ opacity: 0.7, fontSize: 12, margin: 0 }}>
            {isWin
              ? "写入当前用户的注册表（HKCU），无需管理员权限；更改在重启资源管理器或重新登录后稳定生效。"
              : "通过 Launch Services 即时设置；取消关联会还原为系统「归档实用工具」。需应用已被系统识别（安装到「应用程序」）。"}
          </p>
        </>
      )}
    </Modal>
  );
}

// ---------------- Entry properties ----------------

export function EntryProperties(p: {
  entry: {
    name: string;
    path: string;
    isDir: boolean;
    size: number;
    compressed: number;
    mtime: number | null;
    encrypted: boolean;
    crc: number | null;
  };
  onClose: () => void;
}) {
  const e = p.entry;
  const ratio = e.size > 0 ? Math.round((1 - e.compressed / e.size) * 100) : null;
  const rows: [string, React.ReactNode][] = [
    ["名称", e.name],
    ["路径", e.path],
    ["类型", e.isDir ? "文件夹" : "文件"],
    ["原始大小", `${fmtSize(e.size)}（${e.size.toLocaleString()} 字节）`],
  ];
  if (!e.isDir) {
    rows.push(["压缩后", e.compressed > 0 ? fmtSize(e.compressed) : "—"]);
    rows.push(["压缩率", ratio === null ? "—" : `${ratio}%`]);
    rows.push([
      "CRC32",
      e.crc !== null ? e.crc.toString(16).toUpperCase().padStart(8, "0") : "—",
    ]);
  }
  rows.push(["修改时间", fmtDate(e.mtime)]);
  rows.push(["加密", e.encrypted ? "是 🔒" : "否"]);

  return (
    <Modal
      title="属性"
      onClose={p.onClose}
      footer={<button className="btn primary" onClick={p.onClose}>关闭</button>}
    >
      <table className="props">
        <tbody>
          {rows.map(([k, v]) => (
            <tr key={k}>
              <th>{k}</th>
              <td>{v}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </Modal>
  );
}

// ---------------- Preview ----------------

export function PreviewModal(p: {
  archivePath: string;
  entryPath: string;
  password?: string;
  encoding: string;
  onClose: () => void;
}) {
  const [data, setData] = useState<Preview | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    api
      .previewEntry(p.archivePath, p.entryPath, p.password, p.encoding)
      .then(setData)
      .catch((e) => setErr(String(e)));
  }, [p.archivePath, p.entryPath, p.password, p.encoding]);

  const name = p.entryPath.split("/").pop() ?? p.entryPath;

  return (
    <Modal
      title={name}
      wide
      onClose={p.onClose}
      footer={<button className="btn primary" onClick={p.onClose}>关闭</button>}
    >
      {err && <div className="error-text">预览失败：{err}</div>}
      {!data && !err && <div className="hint">加载中…</div>}
      {data && (
        <>
          <div className="hint">
            {fmtSize(data.size)}
            {data.truncated && " · 内容过大，仅显示前 8 MB"}
          </div>
          <div className="preview-body">
            {data.kind === "text" && <pre>{data.text}</pre>}
            {data.kind === "image" && <img src={`data:${data.mime};base64,${data.data}`} alt={name} />}
            {data.kind === "binary" && (
              <pre style={{ color: "var(--text-dim)" }}>二进制文件，无法预览。可解压后用其他应用打开。</pre>
            )}
          </div>
        </>
      )}
    </Modal>
  );
}

// ---------------- Progress ----------------

export function ProgressModal(p: { job: JobState; onCancel: () => void }) {
  const prog = p.job.progress;
  const pct = prog && prog.total > 0 ? Math.min(100, (prog.current / prog.total) * 100) : null;
  return (
    <Modal
      title={p.job.title}
      eyebrow="执行任务"
      description="Origami 正在处理归档内容。"
      icon={p.job.title.includes("解压") ? "extract" : "archive"}
      className="task-dialog progress-dialog"
      footer={<button className="btn" onClick={p.onCancel}>取消</button>}
    >
      <div className="progress-meta">
        <span>{pct === null ? "正在准备" : "处理进度"}</span>
        <strong>{pct === null ? "—" : `${pct.toFixed(0)}%`}</strong>
      </div>
      <div className={`progressbar ${pct === null ? "indeterminate" : ""}`}>
        <div style={{ width: `${pct ?? 30}%` }} />
      </div>
      <div className="progress-file">
        {prog
          ? prog.total > 0
            ? `${fmtSize(prog.current)} / ${fmtSize(prog.total)} · ${prog.file}`
            : `${fmtSize(prog.current)} · ${prog.file}`
          : "准备中…"}
      </div>
    </Modal>
  );
}

// ---------------- Shell (right-click) integration ----------------

export function ShellIntegration(p: { onClose: () => void; toast: (kind: "ok" | "error" | "info", text: string) => void }) {
  const [installed, setInstalled] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [os, setOs] = useState<string>("");

  useEffect(() => {
    api.appPlatform().then(setOs);
    api.shellMenuInstalled().then(setInstalled);
  }, []);

  const isWin = os === "windows";
  const where = isWin ? "资源管理器" : "Finder";

  const doInstall = async () => {
    setBusy(true);
    try {
      await api.installShellMenu();
      setInstalled(true);
      p.toast("ok", `已安装${where}右键菜单`);
    } catch (e) {
      p.toast("error", `安装失败：${e}`);
    } finally {
      setBusy(false);
    }
  };

  const doUninstall = async () => {
    setBusy(true);
    try {
      await api.uninstallShellMenu();
      setInstalled(false);
      p.toast("ok", `已移除${where}右键菜单`);
    } catch (e) {
      p.toast("error", `移除失败：${e}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={`${where} 右键菜单`}
      onClose={p.onClose}
      footer={
        <>
          <button className="btn" onClick={p.onClose}>关闭</button>
          {installed ? (
            <button className="btn" disabled={busy} onClick={doUninstall}>移除</button>
          ) : (
            <button className="btn primary" disabled={busy || installed === null} onClick={doInstall}>
              安装
            </button>
          )}
        </>
      }
    >
      <p style={{ marginTop: 0 }}>
        安装后，在{where}中右键所选文件或文件夹即可压缩：
      </p>
      <ul style={{ lineHeight: 1.9, paddingLeft: 20 }}>
        <li>用 Origami 压缩为 ZIP</li>
        <li>用 Origami 压缩为 7Z</li>
        <li>用 Origami 压缩（详细设置…）— 自定义格式、级别、密码与分卷</li>
      </ul>
      <p style={{ marginBottom: 4 }}>右键压缩包时还可解压：</p>
      <ul style={{ lineHeight: 1.9, paddingLeft: 20 }}>
        <li>解压到当前文件夹（原地解压）</li>
        <li>解压到单独文件夹（同名子目录）</li>
        <li>解压到…（选择位置）</li>
      </ul>
      <p className="hint" style={{ opacity: 0.7, fontSize: 12 }}>
        状态：{installed === null ? "检查中…" : installed ? "✓ 已安装" : "未安装"}。
        {isWin
          ? "Windows 11 上经典菜单位于「显示更多选项」(Shift+F10) 中。"
          : "需要先运行过打包版 Origami.app 以注册 origami:// 链接。"}
      </p>
    </Modal>
  );
}
