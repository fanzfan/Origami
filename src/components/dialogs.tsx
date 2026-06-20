import { useEffect, useMemo, useRef, useState } from "react";
import { api, AssocEntry, fmtDate, fmtSize, Preview, SavedPassword } from "../api";
import type { JobState } from "../App";
import { FONTS, MODES, SCALE_MAX, SCALE_MIN, SCALE_STEP, THEMES, clampScale, type Settings } from "../settings";

function Modal(p: { title: string; wide?: boolean; children: React.ReactNode; footer?: React.ReactNode; onClose?: () => void }) {
  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && p.onClose?.()}>
      <div className={`modal ${p.wide ? "wide" : ""}`}>
        <header>{p.title}</header>
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

  useEffect(() => {
    p.defaultDir().then(setDest);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <Modal
      title={p.entryCount > 0 ? `解压选中的 ${p.entryCount} 项` : "解压全部"}
      onClose={p.onCancel}
      footer={
        <>
          <button className="btn" onClick={p.onCancel}>取消</button>
          <button className="btn primary" disabled={!dest} onClick={() => p.onConfirm(dest, smart)}>
            开始解压
          </button>
        </>
      }
    >
      <div className="field">
        <label>解压到</label>
        <div className="row">
          <input type="text" value={dest} onChange={(e) => setDest(e.target.value)} />
          <button
            className="btn"
            onClick={async () => {
              const d = await p.pickDir();
              if (d) setDest(d);
            }}
          >
            浏览…
          </button>
        </div>
      </div>
      <label className="row" style={{ gap: 8 }}>
        <input type="checkbox" checked={smart} onChange={(e) => setSmart(e.target.checked)} style={{ width: "auto" }} />
        <span>
          智能解压 <span className="hint">（多个顶层文件时自动创建文件夹，避免散落）</span>
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

  return (
    <Modal
      title={`压缩 ${p.sources.length} 项`}
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
            开始压缩
          </button>
        </>
      }
    >
      <div className="hint" style={{ maxHeight: 60, overflow: "auto" }}>
        {p.sources.map((s) => (
          <div key={s}>{s}</div>
        ))}
      </div>

      <div className="field">
        <label>保存为</label>
        <div className="row">
          <input type="text" value={dest} onChange={(e) => setDest(e.target.value)} placeholder="选择保存位置…" />
          <button
            className="btn"
            onClick={async () => {
              const d = await p.pickDest(defName, format);
              if (d) setDest(d);
            }}
          >
            浏览…
          </button>
        </div>
      </div>

      <div className="row">
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

      <div className="field">
        <label>
          压缩等级：{level === 0 ? "仅存储" : level} {level === 9 ? "（最高）" : ""}
        </label>
        <input type="range" min={0} max={9} value={level} onChange={(e) => setLevel(Number(e.target.value))} />
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
      {volume > 0 && <div className="hint">分卷输出为 .001 / .002 … 文件，解压时选择 .001 即可（需先合并或使用支持分卷的工具）。</div>}
    </Modal>
  );
}

// ---------------- Settings ----------------

const LEVEL_HINT: Record<number, string> = {
  0: "仅存储，不压缩（最快）",
  9: "最高压缩率（最慢）",
};

export function SettingsDialog(p: {
  settings: Settings;
  onChange: (patch: Partial<Settings>) => void;
  onClose: () => void;
}) {
  const s = p.settings;
  const pct = Math.round(s.scale * 100);
  const setScale = (v: number) => p.onChange({ scale: clampScale(v) });

  return (
    <Modal
      title="设置"
      onClose={p.onClose}
      footer={<button className="btn primary" onClick={p.onClose}>完成</button>}
    >
      <div className="settings-section">外观</div>

      <div className="field">
        <label>外观模式</label>
        <div className="seg">
          {MODES.map(([key, name]) => (
            <button
              key={key}
              className={`seg-btn ${s.mode === key ? "on" : ""}`}
              onClick={() => p.onChange({ mode: key })}
            >
              {name}
            </button>
          ))}
        </div>
      </div>

      <div className="field">
        <label>主题</label>
        <div className="theme-grid">
          {THEMES.map(([key, name, color]) => (
            <div
              key={key}
              className={`theme-swatch ${s.theme === key ? "on" : ""}`}
              onClick={() => p.onChange({ theme: key })}
            >
              <span className="dot" style={{ background: color }} />
              {name}
            </div>
          ))}
        </div>
      </div>

      <div className="field">
        <label>界面字体大小：{pct}%</label>
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
          <button className="btn sm" onClick={() => setScale(1)}>复位</button>
        </div>
        <div className="hint">快捷键：Ctrl/⌘ 加 + 或 - 调整，Ctrl/⌘ 加 0 复位。</div>
      </div>

      <div className="field">
        <label>字体</label>
        <select value={s.font} onChange={(e) => p.onChange({ font: e.target.value })}>
          {FONTS.map(([key, name]) => (
            <option key={key} value={key}>
              {name}
            </option>
          ))}
        </select>
      </div>

      <label className="toggle-row">
        <input
          type="checkbox"
          checked={s.effects}
          onChange={(e) => p.onChange({ effects: e.target.checked })}
        />
        <span className="grow">
          窗口材质效果
          <div className="hint">Windows 11 显示 Mica 云母材质，macOS 显示毛玻璃；旧系统不支持时自动保持不透明。</div>
        </span>
      </label>

      <div className="settings-section">压缩与解压</div>

      <div className="field">
        <label>
          默认压缩等级：{s.level === 0 ? "仅存储" : s.level}
          {s.level === 9 ? "（最高）" : ""}
        </label>
        <input
          type="range"
          min={0}
          max={9}
          value={s.level}
          onChange={(e) => p.onChange({ level: Number(e.target.value) })}
        />
        <div className="hint">{LEVEL_HINT[s.level] ?? "压缩率与速度的平衡，新建压缩时作为默认值。"}</div>
      </div>

      <label className="toggle-row">
        <input
          type="checkbox"
          checked={s.excludeJunk}
          onChange={(e) => p.onChange({ excludeJunk: e.target.checked })}
        />
        <span className="grow">
          压缩时排除系统垃圾文件
          <div className="hint">自动跳过 .DS_Store、__MACOSX、._ 资源派生、Thumbs.db、desktop.ini 等。</div>
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
  const [list, setList] = useState<SavedPassword[]>([]);
  const [newPw, setNewPw] = useState("");
  const [newLabel, setNewLabel] = useState("");
  const [reveal, setReveal] = useState(false);
  const [phase, setPhase] = useState<AuthPhase>("checking");

  const refresh = () => api.pwList().then(setList);

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
        打开或解压加密归档时，会自动按最近使用顺序尝试这些密码。
      </p>
      <div className="row">
        <input type={reveal ? "text" : "password"} placeholder="新密码" value={newPw} onChange={(e) => setNewPw(e.target.value)} />
        <input type="text" placeholder="备注（可选）" value={newLabel} onChange={(e) => setNewLabel(e.target.value)} />
        <button
          className="btn"
          disabled={!newPw}
          onClick={async () => {
            await api.pwAdd(newPw, newLabel || undefined);
            setNewPw("");
            setNewLabel("");
            refresh();
          }}
        >
          添加
        </button>
      </div>
      <label className="row" style={{ gap: 8 }}>
        <input type="checkbox" checked={reveal} onChange={(e) => setReveal(e.target.checked)} style={{ width: "auto" }} />
        <span className="hint">显示密码</span>
      </label>
      <div className="pwlist">
        {list.length === 0 && <div className="hint">还没有保存的密码</div>}
        {list.map((e) => (
          <div className="pw-item" key={e.password}>
            <code>{reveal ? e.password : "•".repeat(Math.min(e.password.length, 12))}</code>
            {e.label && <span className="label">{e.label}</span>}
            <button
              className="btn sm danger"
              onClick={async () => {
                await api.pwRemove(e.password);
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
    <div className="overlay">
      <div className="modal">
        <header>{p.job.title}</header>
        <div className="body">
          <div className={`progressbar ${pct === null ? "indeterminate" : ""}`}>
            <div style={{ width: `${pct ?? 30}%` }} />
          </div>
          <div className="progress-file">
            {prog
              ? prog.total > 0
                ? `${pct!.toFixed(0)}% · ${fmtSize(prog.current)} / ${fmtSize(prog.total)} · ${prog.file}`
                : `${fmtSize(prog.current)} · ${prog.file}`
              : "准备中…"}
          </div>
        </div>
        <footer>
          <button className="btn" onClick={p.onCancel}>取消</button>
        </footer>
      </div>
    </div>
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
        安装后，在{where}中右键所选文件或文件夹即可使用：
      </p>
      <ul style={{ lineHeight: 1.9, paddingLeft: 20 }}>
        <li>用 Origami 压缩为 ZIP</li>
        <li>用 Origami 压缩为 7Z</li>
        <li>用 Origami 压缩（详细设置…）— 自定义格式、级别、密码与分卷</li>
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
