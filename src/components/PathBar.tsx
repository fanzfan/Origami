import { useEffect, useRef, useState, type MouseEvent, type ReactNode } from "react";

// Windows 资源管理器式地址栏：平时显示可点击的面包屑；点击空白区域切换为可编辑的
// 文本输入（显示完整路径），回车跳转、Esc/失焦取消。输入框右键沿用 WebView 的系统
// 编辑菜单（撤销/剪切/复制/粘贴/全选）。
export interface Crumb {
  label: string;
  onClick: () => void;
}

export function PathBar(p: {
  crumbs: Crumb[];
  fullPath: string; // 编辑态预填的完整路径文本
  onSubmit: (value: string) => void;
  trailing?: ReactNode; // 右侧附加操作（如全选/反选）
}) {
  const [editing, setEditing] = useState(false);
  const [val, setVal] = useState(p.fullPath);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!editing) return;
    setVal(p.fullPath);
    // 进入编辑态后聚焦并全选，便于直接覆盖输入或复制。
    requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    });
  }, [editing, p.fullPath]);

  const commit = () => {
    const v = val.trim();
    setEditing(false);
    if (v && v !== p.fullPath) p.onSubmit(v);
  };

  if (editing) {
    return (
      <div className="breadcrumb editing">
        <input
          ref={inputRef}
          className="pathedit"
          value={val}
          spellCheck={false}
          onChange={(e) => setVal(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") commit();
            else if (e.key === "Escape") setEditing(false);
          }}
          onBlur={() => setEditing(false)}
        />
      </div>
    );
  }

  // 空白处（含容器自身留白与 pathspace 填充块）点击进入编辑态。
  const blankToEdit = (e: MouseEvent) => {
    const t = e.target as HTMLElement;
    if (t === e.currentTarget || t.classList.contains("pathspace")) setEditing(true);
  };

  return (
    <div className="breadcrumb" onClick={blankToEdit} title="点击空白处编辑路径">
      {p.crumbs.map((c, i) => (
        <span key={i} style={{ display: "inline-flex", alignItems: "center", gap: 2 }}>
          {i > 0 && <span className="sep">›</span>}
          <span
            className={`crumb ${i === p.crumbs.length - 1 ? "current" : ""}`}
            onClick={(e) => { e.stopPropagation(); c.onClick(); }}
          >
            {c.label}
          </span>
        </span>
      ))}
      <span className="pathspace" />
      {p.trailing}
    </div>
  );
}
