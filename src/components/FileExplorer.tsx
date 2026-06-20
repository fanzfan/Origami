import { useEffect, useMemo, useState } from "react";
import { DirListing, FsEntry, fmtDate, fmtSize, fsBreadcrumbs, isArchive } from "../api";
import { FileIcon } from "../icons";
import { PathBar } from "./PathBar";

// 应用内文件管理器（文件系统视图）。浏览任意真实路径；双击文件夹进入、双击压缩包
// 像进文件夹一样进入其内部、双击普通文件用默认程序打开。右键菜单为「压缩」（与压缩包
// 内的「解压」相对）。
interface Props {
  listing: DirListing;
  loading: boolean;
  onNavigate: (path?: string) => void; // path 省略 = 主目录；"" = 此电脑
  onOpenArchive: (path: string) => void;
  onOpenFile: (path: string) => void;
  onCompress: (paths: string[]) => void;
}

type SortKey = "name" | "size" | "mtime";

export function FileExplorer(p: Props) {
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [sortAsc, setSortAsc] = useState(true);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [lastClicked, setLastClicked] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);

  // 切换目录时清空选择。
  useEffect(() => {
    setSelected(new Set());
    setLastClicked(null);
  }, [p.listing.path]);

  const rows = useMemo(() => {
    const dir = sortAsc ? 1 : -1;
    const all = [...p.listing.entries];
    all.sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      switch (sortKey) {
        case "size": return (a.size - b.size) * dir;
        case "mtime": return ((a.mtime ?? 0) - (b.mtime ?? 0)) * dir;
        default: return a.name.localeCompare(b.name, "zh") * dir;
      }
    });
    return all;
  }, [p.listing.entries, sortKey, sortAsc]);

  const clickSort = (k: SortKey) => {
    if (sortKey === k) setSortAsc(!sortAsc);
    else { setSortKey(k); setSortAsc(true); }
  };
  const sortIcon = (k: SortKey) => (sortKey === k ? (sortAsc ? " ↑" : " ↓") : "");

  const toggleSelect = (row: FsEntry, e: React.MouseEvent) => {
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

  const onDouble = (row: FsEntry) => {
    if (row.isDir) p.onNavigate(row.path);
    else if (isArchive(row.path)) p.onOpenArchive(row.path);
    else p.onOpenFile(row.path);
  };

  const openMenu = (row: FsEntry, e: React.MouseEvent) => {
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

  const crumbs = useMemo(() => fsBreadcrumbs(p.listing.path), [p.listing.path]);
  const selCount = selected.size;
  const menuRow = useMemo(
    () => (selected.size === 1 ? rows.find((r) => selected.has(r.path)) ?? null : null),
    [rows, selected],
  );
  const selectedBytes = useMemo(
    () => rows.filter((r) => selected.has(r.path)).reduce((a, r) => a + r.size, 0),
    [rows, selected],
  );

  return (
    <div className="browser">
      <div className="toolbar">
        <button
          className="btn"
          disabled={p.listing.parent === null}
          onClick={() => p.listing.parent !== null && p.onNavigate(p.listing.parent)}
          title="上级目录（Backspace）"
        >
          ↑ 上级
        </button>
        <button
          className="btn primary"
          disabled={selCount === 0}
          onClick={() => p.onCompress([...selected])}
          title="把选中项压缩为压缩包"
        >
          🗜 压缩 {selCount > 0 ? `(${selCount})` : ""}
        </button>
        <span className="spacer" />
      </div>

      <PathBar
        crumbs={crumbs.map((c) => ({ label: c.label, onClick: () => p.onNavigate(c.path) }))}
        fullPath={p.listing.path}
        onSubmit={(v) => p.onNavigate(v)}
      />

      <div className="filelist">
        <table className="files">
          <thead>
            <tr>
              <th onClick={() => clickSort("name")}>名称{sortIcon("name")}</th>
              <th onClick={() => clickSort("size")} style={{ width: 110, textAlign: "right" }}>
                大小{sortIcon("size")}
              </th>
              <th onClick={() => clickSort("mtime")} style={{ width: 160 }}>
                修改时间{sortIcon("mtime")}
              </th>
            </tr>
          </thead>
          <tbody>
            {p.listing.parent !== null && (
              <tr onDoubleClick={() => p.onNavigate(p.listing.parent!)}>
                <td className="name">
                  <span className="icon">↩️</span>
                  <span>..</span>
                </td>
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
                onContextMenu={(e) => openMenu(r, e)}
              >
                <td className="name">
                  <FileIcon name={r.name} isDir={r.isDir} />
                  <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>{r.name}</span>
                </td>
                <td className="num">{r.isDir ? "—" : fmtSize(r.size)}</td>
                <td style={{ color: "var(--text-dim)" }}>{fmtDate(r.mtime)}</td>
              </tr>
            ))}
            {rows.length === 0 && (
              <tr>
                <td colSpan={3} style={{ textAlign: "center", padding: 30, color: "var(--text-dim)" }}>
                  {p.loading ? "加载中…" : "空目录"}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <div className="statusbar">
        {selCount > 0 ? (
          <span>已选 {selCount} 项{selectedBytes > 0 ? ` · ${fmtSize(selectedBytes)}` : ""}</span>
        ) : (
          <span>{p.listing.entries.length} 个项目</span>
        )}
        <span className="spacer" />
        <span title={p.listing.path}>{p.listing.path || "此电脑"}</span>
      </div>

      {menu && (
        <div
          className="ctxmenu"
          style={{ left: menu.x, top: menu.y }}
          onClick={(e) => e.stopPropagation()}
          onContextMenu={(e) => e.preventDefault()}
        >
          <button onClick={() => { p.onCompress([...selected]); setMenu(null); }}>
            压缩{selCount > 1 ? `选中 (${selCount})` : "…"}
          </button>
          {menuRow && !menuRow.isDir && isArchive(menuRow.path) && (
            <button onClick={() => { p.onOpenArchive(menuRow.path); setMenu(null); }}>
              在 Origami 中打开
            </button>
          )}
          {menuRow && !menuRow.isDir && (
            <button onClick={() => { p.onOpenFile(menuRow.path); setMenu(null); }}>
              用默认程序打开
            </button>
          )}
          {menuRow && menuRow.isDir && (
            <button onClick={() => { p.onNavigate(menuRow.path); setMenu(null); }}>
              打开文件夹
            </button>
          )}
          <div className="sep" />
          <button onClick={() => { copy([...selected].join("\n")); setMenu(null); }}>
            复制路径{selCount > 1 ? ` (${selCount})` : ""}
          </button>
        </div>
      )}
    </div>
  );
}
