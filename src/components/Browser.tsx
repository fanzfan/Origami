import { useMemo, useState } from "react";
import { ArchiveInfo, Entry, fmtDate, fmtSize } from "../api";
import { FileIcon } from "../icons";

interface Props {
  info: ArchiveInfo;
  archivePath: string;
  loading: boolean;
  encoding: string;
  onEncodingChange: (enc: string) => void;
  onExtract: (entries: string[]) => void;
  onTest: () => void;
  onPreview: (entryPath: string) => void;
  onAdd: (dir: string) => void;
  onRemove: (entries: string[]) => void;
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
  entry: Entry | null; // null for implicit dirs
}

type SortKey = "name" | "size" | "mtime";

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

export function Browser(p: Props) {
  const [cwd, setCwd] = useState("");
  const [search, setSearch] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortAsc, setSortAsc] = useState(true);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [lastClicked, setLastClicked] = useState<string | null>(null);

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
              mtime: e.mtime, encrypted: false, entry: e,
            });
        } else {
          files.push({
            name: rest, path: norm, isDir: false, size: e.size, compressed: e.compressed,
            mtime: e.mtime, encrypted: e.encrypted, entry: e,
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
            mtime: null, encrypted: false, entry: null,
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
        case "mtime": return ((a.mtime ?? 0) - (b.mtime ?? 0)) * dir;
        default: return a.name.localeCompare(b.name, "zh") * dir;
      }
    });
    return all;
  }, [p.info, cwd, search, sortKey, sortAsc]);

  const crumbs = useMemo(() => {
    const parts = cwd ? cwd.split("/") : [];
    const list: { label: string; path: string }[] = [{ label: "📦", path: "" }];
    let acc = "";
    for (const part of parts) {
      acc = acc ? `${acc}/${part}` : part;
      list.push({ label: part, path: acc });
    }
    return list;
  }, [cwd]);

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

  const onDouble = (row: Row) => {
    if (row.isDir) {
      setSearch("");
      setCwd(row.path);
      setSelected(new Set());
    } else {
      p.onPreview(row.path);
    }
  };

  const selCount = selected.size;
  const sortIcon = (k: SortKey) => (sortKey === k ? (sortAsc ? " ↑" : " ↓") : "");

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
        {EDITABLE.has(p.info.format) && (
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
              title="从压缩包中删除选中条目"
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
          className="search"
          type="text"
          placeholder="搜索归档内文件…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </div>

      <div className="breadcrumb">
        {crumbs.map((c, i) => (
          <span key={c.path} style={{ display: "inline-flex", alignItems: "center", gap: 2 }}>
            {i > 0 && <span className="sep">›</span>}
            <span
              className={`crumb ${i === crumbs.length - 1 ? "current" : ""}`}
              onClick={() => {
                setCwd(c.path);
                setSelected(new Set());
              }}
            >
              {c.label}
            </span>
          </span>
        ))}
      </div>

      <div className="filelist">
        <table className="files">
          <thead>
            <tr>
              <th onClick={() => clickSort("name")}>名称{sortIcon("name")}</th>
              <th onClick={() => clickSort("size")} style={{ width: 90, textAlign: "right" }}>
                大小{sortIcon("size")}
              </th>
              <th style={{ width: 90, textAlign: "right" }}>压缩后</th>
              <th onClick={() => clickSort("mtime")} style={{ width: 140 }}>
                修改时间{sortIcon("mtime")}
              </th>
            </tr>
          </thead>
          <tbody>
            {cwd && !search && (
              <tr
                onDoubleClick={() => {
                  const up = cwd.includes("/") ? cwd.slice(0, cwd.lastIndexOf("/")) : "";
                  setCwd(up);
                  setSelected(new Set());
                }}
              >
                <td className="name">
                  <span className="icon">↩️</span>
                  <span>..</span>
                </td>
                <td className="num" />
                <td className="num" />
                <td />
              </tr>
            )}
            {rows.map((r) => (
              <tr
                key={r.path}
                className={selected.has(r.path) ? "sel" : ""}
                onClick={(e) => toggleSelect(r, e)}
                onDoubleClick={() => onDouble(r)}
              >
                <td className="name">
                  <FileIcon name={r.name} isDir={r.isDir} />
                  <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{r.name}</span>
                  {r.encrypted && <span className="lock">🔒</span>}
                </td>
                <td className="num">{r.isDir && r.size === 0 ? "—" : fmtSize(r.size)}</td>
                <td className="num">{r.compressed > 0 ? fmtSize(r.compressed) : "—"}</td>
                <td style={{ color: "var(--text-dim)" }}>{fmtDate(r.mtime)}</td>
              </tr>
            ))}
            {rows.length === 0 && (
              <tr>
                <td colSpan={4} style={{ textAlign: "center", padding: 30, color: "var(--text-dim)" }}>
                  {search ? "没有匹配的文件" : "空目录"}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <div className="statusbar">
        <span>
          {p.info.entries.length} 个条目 · 原始 {fmtSize(p.info.totalSize)} · 压缩后{" "}
          {fmtSize(p.info.totalCompressed)}
          {p.info.totalSize > 0 &&
            ` · 压缩率 ${Math.round((1 - p.info.totalCompressed / p.info.totalSize) * 100)}%`}
        </span>
        <span className="spacer" />
        {p.info.hasEncrypted && <span>🔒 含加密内容</span>}
        {p.info.comment && <span title={p.info.comment}>💬 含注释</span>}
        <span>{p.info.format}</span>
      </div>
    </div>
  );
}
