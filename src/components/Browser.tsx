import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArchiveInfo, Entry, fmtDate, fmtSize, fsBreadcrumbs, splitParent } from "../api";
import { FileIcon } from "../icons";
import { EntryProperties } from "./dialogs";
import { PathBar } from "./PathBar";

interface Props {
  info: ArchiveInfo;
  archivePath: string;
  loading: boolean;
  encoding: string;
  onEncodingChange: (enc: string) => void;
  onExtract: (entries: string[]) => void;
  onTest: () => void;
  onPreview: (entryPath: string) => void;
  onOpenExternal: (entryPath: string) => void;
  onAdd: (dir: string) => void;
  onRemove: (entries: string[]) => void;
  // 导航到某个真实文件系统目录（离开压缩包）。"" = 此电脑。由路径栏的文件系统段
  // 与压缩包根的 .. 触发。
  onNavigateFs?: (path: string) => void;
}

const EDITABLE = new Set(["ZIP", "7Z", "TAR", "TAR.GZ", "TAR.BZ2", "TAR.XZ", "TAR.ZST"]);

interface Row {
  name: string;
  path: string; // full virtual path
  isDir: boolean;
  size: number;
  compressed: number;
  mtime: number | null;
  encrypted: boolean;
  crc: number | null;
  entry: Entry | null; // null for implicit dirs
}

type SortKey = "name" | "size" | "ratio" | "mtime";

const ENCODINGS: [string, string][] = [
  ["auto", "自动检测"],
  ["utf-8", "UTF-8"],
  ["gbk", "简体中文 (GBK)"],
  ["big5", "繁體中文 (Big5)"],
  ["shift_jis", "日本語 (Shift-JIS)"],
  ["euc-kr", "한국어 (EUC-KR)"],
  ["windows-1251", "Кириллица (CP1251)"],
  ["cp437", "DOS (CP437)"],
];

function ratioOf(r: { size: number; compressed: number }): number | null {
  return r.size > 0 ? Math.round((1 - r.compressed / r.size) * 100) : null;
}

export function Browser(p: Props) {
  const [cwd, setCwd] = useState("");
  const [search, setSearch] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortAsc, setSortAsc] = useState(true);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [lastClicked, setLastClicked] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [propsRow, setPropsRow] = useState<Row | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  const editable = EDITABLE.has(p.info.format);

  const rows = useMemo(() => {
    const prefix = cwd ? cwd + "/" : "";
    const dirMap = new Map<string, Row>();
    const files: Row[] = [];
    const q = search.trim().toLowerCase();

    for (const e of p.info.entries) {
      const norm = e.path.replace(/\/+$/, "");
      if (q) {
        // search mode: flat results across the archive
        if (!norm.toLowerCase().includes(q)) continue;
        files.push({
          name: norm,
          path: norm,
          isDir: e.isDir,
          size: e.size,
          compressed: e.compressed,
          mtime: e.mtime,
          encrypted: e.encrypted,
          crc: e.crc,
          entry: e,
        });
        continue;
      }
      if (!norm.startsWith(prefix) || norm === cwd) continue;
      const rest = norm.slice(prefix.length);
      const slash = rest.indexOf("/");
      if (slash === -1) {
        if (e.isDir) {
          if (!dirMap.has(rest))
            dirMap.set(rest, {
              name: rest, path: norm, isDir: true, size: 0, compressed: 0,
              mtime: e.mtime, encrypted: false, crc: null, entry: e,
            });
        } else {
          files.push({
            name: rest, path: norm, isDir: false, size: e.size, compressed: e.compressed,
            mtime: e.mtime, encrypted: e.encrypted, crc: e.crc, entry: e,
          });
        }
      } else {
        const dir = rest.slice(0, slash);
        const existing = dirMap.get(dir);
        if (existing) {
          existing.size += e.size;
          existing.compressed += e.compressed;
        } else {
          dirMap.set(dir, {
            name: dir, path: prefix + dir, isDir: true, size: e.size, compressed: e.compressed,
            mtime: null, encrypted: false, crc: null, entry: null,
          });
        }
      }
    }

    const all = [...dirMap.values(), ...files];
    const dir = sortAsc ? 1 : -1;
    all.sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      switch (sortKey) {
        case "size": return (a.size - b.size) * dir;
        case "ratio": return ((ratioOf(a) ?? -1) - (ratioOf(b) ?? -1)) * dir;
        case "mtime": return ((a.mtime ?? 0) - (b.mtime ?? 0)) * dir;
        default: return a.name.localeCompare(b.name, "zh") * dir;
      }
    });
    return all;
  }, [p.info, cwd, search, sortKey, sortAsc]);

  // 压缩包所在真实文件夹（用于把路径栏拼成「真实路径 › 压缩包 › 内部目录」）。
  const archiveParent = useMemo(() => splitParent(p.archivePath), [p.archivePath]);

  // 统一路径栏：文件系统段（此电脑…文件夹）+ 压缩包本身（当作一个文件夹）+ 包内子目录。
  type Crumb = { label: string; kind: "fs" | "arc" | "int"; target: string };
  const crumbs = useMemo(() => {
    const list: Crumb[] = [];
    if (p.onNavigateFs) {
      for (const c of fsBreadcrumbs(archiveParent.parent)) {
        list.push({ label: c.label, kind: "fs", target: c.path });
      }
    }
    list.push({ label: `🗜 ${archiveParent.name}`, kind: "arc", target: "" });
    let acc = "";
    for (const part of cwd ? cwd.split("/") : []) {
      acc = acc ? `${acc}/${part}` : part;
      list.push({ label: part, kind: "int", target: acc });
    }
    return list;
  }, [cwd, archiveParent, p.onNavigateFs]);

  const onCrumb = useCallback(
    (c: Crumb) => {
      if (c.kind === "fs") p.onNavigateFs?.(c.target);
      else enterDir(c.target); // arc 根("") 或包内子目录
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [p.onNavigateFs],
  );

  // 编辑态预填的完整路径：真实路径 + 包名 + 包内子目录。
  const fullPath = useMemo(() => {
    const sep = p.archivePath.includes("\\") ? "\\" : "/";
    return p.archivePath + (cwd ? sep + cwd.split("/").join(sep) : "");
  }, [p.archivePath, cwd]);

  const clickSort = (k: SortKey) => {
    if (sortKey === k) setSortAsc(!sortAsc);
    else {
      setSortKey(k);
      setSortAsc(true);
    }
  };

  const toggleSelect = (row: Row, e: React.MouseEvent) => {
    setLastClicked(row.path);
    setSelected((sel) => {
      const next = new Set(sel);
      if (e.metaKey || e.ctrlKey) {
        next.has(row.path) ? next.delete(row.path) : next.add(row.path);
        return next;
      }
      if (e.shiftKey && lastClicked) {
        const paths = rows.map((r) => r.path);
        const a = paths.indexOf(lastClicked);
        const b = paths.indexOf(row.path);
        if (a !== -1 && b !== -1) {
          for (let i = Math.min(a, b); i <= Math.max(a, b); i++) next.add(paths[i]);
          return next;
        }
      }
      return new Set([row.path]);
    });
  };

  const enterDir = useCallback((path: string) => {
    setSearch("");
    setCwd(path);
    setSelected(new Set());
  }, []);

  const goUp = useCallback(() => {
    if (!cwd) {
      // 已在压缩包根：退回到压缩包所在的真实文件夹。
      p.onNavigateFs?.(archiveParent.parent);
      return;
    }
    enterDir(cwd.includes("/") ? cwd.slice(0, cwd.lastIndexOf("/")) : "");
  }, [cwd, enterDir, p, archiveParent]);

  const onDouble = (row: Row) => {
    if (row.isDir) enterDir(row.path);
    else p.onPreview(row.path);
  };

  const selCount = selected.size;
  const sortIcon = (k: SortKey) => (sortKey === k ? (sortAsc ? " ↑" : " ↓") : "");

  const selectAll = useCallback(() => setSelected(new Set(rows.map((r) => r.path))), [rows]);
  const invert = useCallback(
    () => setSelected((sel) => new Set(rows.filter((r) => !sel.has(r.path)).map((r) => r.path))),
    [rows],
  );

  const selectedFiles = useMemo(
    () => rows.filter((r) => selected.has(r.path) && !r.isDir),
    [rows, selected],
  );
  const selectedBytes = useMemo(
    () => rows.filter((r) => selected.has(r.path)).reduce((a, r) => a + r.size, 0),
    [rows, selected],
  );

  // 右键：若目标不在已选集合中，则先单选它，再弹菜单。
  const openMenu = (row: Row, e: React.MouseEvent) => {
    e.preventDefault();
    if (!selected.has(row.path)) {
      setSelected(new Set([row.path]));
      setLastClicked(row.path);
    }
    setMenu({ x: e.clientX, y: e.clientY });
  };

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    window.addEventListener("click", close);
    window.addEventListener("scroll", close, true);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("scroll", close, true);
    };
  }, [menu]);

  const copy = (text: string) => navigator.clipboard?.writeText(text).catch(() => {});

  // 键盘快捷键：作用于文件列表，输入框聚焦时不拦截（除 Esc）。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      const typing = tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA";
      const mod = e.metaKey || e.ctrlKey;

      if (e.key === "Escape") {
        if (search) setSearch("");
        else if (selected.size) setSelected(new Set());
        return;
      }
      if (mod && (e.key === "f" || e.key === "F")) {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
        return;
      }
      if (typing) return;
      if (mod && (e.key === "a" || e.key === "A")) {
        e.preventDefault();
        selectAll();
        return;
      }
      if (e.key === "Backspace") {
        e.preventDefault();
        goUp();
        return;
      }
      if (e.key === "Enter" && selected.size === 1) {
        e.preventDefault();
        const row = rows.find((r) => selected.has(r.path));
        if (row) onDouble(row);
        return;
      }
      if (e.key === " " && selected.size === 1) {
        const row = rows.find((r) => selected.has(r.path));
        if (row && !row.isDir) {
          e.preventDefault();
          p.onPreview(row.path);
        }
        return;
      }
      if ((e.key === "Delete" || (e.metaKey && e.key === "Backspace")) && editable && selected.size) {
        e.preventDefault();
        p.onRemove([...selected]);
        setSelected(new Set());
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, selected, search, editable, goUp, selectAll]);

  const menuTargetRow = useMemo(() => {
    if (selected.size !== 1) return null;
    return rows.find((r) => selected.has(r.path)) ?? null;
  }, [rows, selected]);

  return (
    <div className="browser">
      <div className="toolbar">
        <button className="btn primary" onClick={() => p.onExtract([])}>
          ⬇ 解压全部
        </button>
        <button className="btn" disabled={selCount === 0} onClick={() => p.onExtract([...selected])}>
          解压选中 {selCount > 0 ? `(${selCount})` : ""}
        </button>
        <button className="btn" onClick={p.onTest}>
          ✓ 校验
        </button>
        {editable && (
          <>
            <button className="btn" onClick={() => p.onAdd(cwd)} title="添加文件到当前目录">
              ＋ 添加
            </button>
            <button
              className="btn danger"
              disabled={selCount === 0}
              onClick={() => {
                p.onRemove([...selected]);
                setSelected(new Set());
              }}
              title="从压缩包中删除选中条目（Delete）"
            >
              − 删除 {selCount > 0 ? `(${selCount})` : ""}
            </button>
          </>
        )}
        <span className="spacer" />
        <select
          value={p.encoding}
          onChange={(e) => p.onEncodingChange(e.target.value)}
          style={{ width: 160 }}
          title="文件名编码"
        >
          {ENCODINGS.map(([v, l]) => (
            <option key={v} value={v}>
              {l}
            </option>
          ))}
        </select>
        <input
          ref={searchRef}
          className="search"
          type="text"
          placeholder="搜索归档内文件…（⌘/Ctrl+F）"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </div>

      <PathBar
        crumbs={crumbs.map((c) => ({ label: c.label, onClick: () => onCrumb(c) }))}
        fullPath={fullPath}
        onSubmit={(v) => p.onNavigateFs?.(v)}
        trailing={
          <>
            <button className="linkbtn" onClick={selectAll} title="全选（⌘/Ctrl+A）">全选</button>
            <button className="linkbtn" onClick={invert} title="反选">反选</button>
          </>
        }
      />

      <div className="filelist">
        <table className="files">
          <thead>
            <tr>
              <th onClick={() => clickSort("name")}>名称{sortIcon("name")}</th>
              <th onClick={() => clickSort("size")} style={{ width: 90, textAlign: "right" }}>
                大小{sortIcon("size")}
              </th>
              <th style={{ width: 90, textAlign: "right" }}>压缩后</th>
              <th onClick={() => clickSort("ratio")} style={{ width: 64, textAlign: "right" }}>
                压缩率{sortIcon("ratio")}
              </th>
              <th onClick={() => clickSort("mtime")} style={{ width: 140 }}>
                修改时间{sortIcon("mtime")}
              </th>
            </tr>
          </thead>
          <tbody>
            {(cwd || p.onNavigateFs) && !search && (
              <tr onDoubleClick={goUp}>
                <td className="name">
                  <span className="icon">↩️</span>
                  <span>{cwd ? ".." : ".. (返回上级文件夹)"}</span>
                </td>
                <td className="num" />
                <td className="num" />
                <td className="num" />
                <td />
              </tr>
            )}
            {rows.map((r) => {
              const ratio = ratioOf(r);
              return (
                <tr
                  key={r.path}
                  className={selected.has(r.path) ? "sel" : ""}
                  onClick={(e) => toggleSelect(r, e)}
                  onDoubleClick={() => onDouble(r)}
                  onContextMenu={(e) => openMenu(r, e)}
                >
                  <td className="name">
                    <FileIcon name={r.name} isDir={r.isDir} />
                    <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{r.name}</span>
                    {r.encrypted && <span className="lock">🔒</span>}
                  </td>
                  <td className="num">{r.isDir && r.size === 0 ? "—" : fmtSize(r.size)}</td>
                  <td className="num">{r.compressed > 0 ? fmtSize(r.compressed) : "—"}</td>
                  <td className="num" style={{ color: "var(--text-dim)" }}>
                    {ratio === null ? "—" : `${ratio}%`}
                  </td>
                  <td style={{ color: "var(--text-dim)" }}>{fmtDate(r.mtime)}</td>
                </tr>
              );
            })}
            {rows.length === 0 && (
              <tr>
                <td colSpan={5} style={{ textAlign: "center", padding: 30, color: "var(--text-dim)" }}>
                  {search ? "没有匹配的文件" : "空目录"}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <div className="statusbar">
        {selCount > 0 ? (
          <span>
            已选 {selCount} 项{selectedFiles.length > 0 ? ` · ${fmtSize(selectedBytes)}` : ""}
          </span>
        ) : (
          <span>
            {p.info.entries.length} 个条目 · 原始 {fmtSize(p.info.totalSize)} · 压缩后{" "}
            {fmtSize(p.info.totalCompressed)}
            {p.info.totalSize > 0 &&
              ` · 压缩率 ${Math.round((1 - p.info.totalCompressed / p.info.totalSize) * 100)}%`}
          </span>
        )}
        <span className="spacer" />
        {p.info.hasEncrypted && <span>🔒 含加密内容</span>}
        {p.info.comment && <span title={p.info.comment}>💬 含注释</span>}
        <span>{p.info.format}</span>
      </div>

      {menu && (
        <div
          className="ctxmenu"
          style={{ left: menu.x, top: menu.y }}
          onClick={(e) => e.stopPropagation()}
          onContextMenu={(e) => e.preventDefault()}
        >
          {menuTargetRow && !menuTargetRow.isDir && (
            <>
              <button onClick={() => { p.onOpenExternal(menuTargetRow.path); setMenu(null); }}>
                用默认程序打开
              </button>
              <button onClick={() => { p.onPreview(menuTargetRow.path); setMenu(null); }}>
                预览
              </button>
              <div className="sep" />
            </>
          )}
          <button onClick={() => { p.onExtract([...selected]); setMenu(null); }}>
            解压{selCount > 1 ? `选中 (${selCount})` : "…"}
          </button>
          <div className="sep" />
          {menuTargetRow && (
            <button onClick={() => { copy(menuTargetRow.name); setMenu(null); }}>复制名称</button>
          )}
          <button
            onClick={() => { copy([...selected].join("\n")); setMenu(null); }}
          >
            复制路径{selCount > 1 ? ` (${selCount})` : ""}
          </button>
          {menuTargetRow && (
            <button onClick={() => { setPropsRow(menuTargetRow); setMenu(null); }}>属性</button>
          )}
          {editable && (
            <>
              <div className="sep" />
              <button
                className="danger"
                onClick={() => { p.onRemove([...selected]); setSelected(new Set()); setMenu(null); }}
              >
                从压缩包删除{selCount > 1 ? ` (${selCount})` : ""}
              </button>
            </>
          )}
        </div>
      )}

      {propsRow && <EntryProperties entry={propsRow} onClose={() => setPropsRow(null)} />}
    </div>
  );
}
