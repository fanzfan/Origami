import { useEffect, useState, type SVGProps } from "react";
import { api } from "./api";

export type UiIconName =
  | "brand"
  | "home"
  | "integration"
  | "link"
  | "key"
  | "settings"
  | "close"
  | "archive"
  | "folder-open"
  | "file-archive"
  | "folder-archive"
  | "extract"
  | "verify"
  | "add"
  | "delete"
  | "up"
  | "lock"
  | "layers"
  | "comment"
  | "folder"
  | "file";

interface UiIconProps extends Omit<SVGProps<SVGSVGElement>, "name"> {
  name: UiIconName;
  size?: number;
}

/** 界面用单色线性图标。所有颜色继承 currentColor，便于在各主题下统一着色。 */
export function UiIcon({ name, size = 16, className = "", ...props }: UiIconProps) {
  let paths: React.ReactNode;

  switch (name) {
    case "brand":
      paths = (
        <>
          <path d="m2.8 14.5 5.2-.7L9.6 3.4l6.1 8.1 3.4-4 2.4 2.1-3.2.8-.5 7-3-2.8-2.5 5.5-3.4-3.1-3.4 1.7Z" />
          <path d="m8 13.8 4.3-1.4 2.5 2.2m-2.5-2.2v7.7m3.4-8.6 2.6-1.1" />
        </>
      );
      break;
    case "home":
      paths = <><path d="m3 11 9-7.5 9 7.5" /><path d="M5.5 9.5V21h13V9.5M9.5 21v-6h5v6" /></>;
      break;
    case "integration":
      paths = <path d="M9.2 3.5H4.5v5a2.5 2.5 0 1 1 0 5v5h5a2.5 2.5 0 1 0 5 0h5v-5a2.5 2.5 0 1 0 0-5v-5h-5a2.5 2.5 0 1 1-5 0Z" />;
      break;
    case "link":
      paths = <><path d="m9.5 14.5 5-5" /><path d="m7.2 17.8-1.1 1.1a3.5 3.5 0 0 1-5-5l3.3-3.3a3.5 3.5 0 0 1 5 0" /><path d="m16.8 6.2 1.1-1.1a3.5 3.5 0 0 1 5 5l-3.3 3.3a3.5 3.5 0 0 1-5 0" /></>;
      break;
    case "key":
      paths = <><circle cx="8" cy="9" r="4.5" /><path d="m11.2 12.2 8.3 8.3m-3-3 2.2-2.2m-5.2-.8 2.2-2.2" /></>;
      break;
    case "settings":
      paths = <><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H2.8v-4H3a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1a1.7 1.7 0 0 0 1.9.3A1.7 1.7 0 0 0 10 3V2.8h4V3a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4H21a1.7 1.7 0 0 0-1.6 1Z" /></>;
      break;
    case "close":
      paths = <path d="m6 6 12 12M18 6 6 18" />;
      break;
    case "archive":
      paths = <><path d="M4 7.5h16V21H4zM3 3h18v4.5H3z" /><path d="M10 11h4" /></>;
      break;
    case "folder-open":
      paths = <><path d="M3 19.5V5h7l2 2h9v3" /><path d="M3 19.5h16l2.5-9.5h-16Z" /></>;
      break;
    case "file-archive":
      paths = <><path d="M6 2.5h8l4 4V21H6zM14 2.5v4h4" /><path d="M12 9v8m-2-6h2m0 3h2" /></>;
      break;
    case "folder-archive":
      paths = <><path d="M3 19.5V5h7l2 2h9v12.5Z" /><path d="M15 10v7m-2-5h2m0 3h2" /></>;
      break;
    case "extract":
      paths = <><path d="M4 12V5h16v7M4 16v5h16v-5" /><path d="M12 3v13m-4-4 4 4 4-4" /></>;
      break;
    case "verify":
      paths = <><circle cx="12" cy="12" r="9" /><path d="m8 12.2 2.6 2.6 5.7-6" /></>;
      break;
    case "add":
      paths = <><path d="M4 19.5V5h7l2 2h7v12.5Z" /><path d="M12 10v6m-3-3h6" /></>;
      break;
    case "delete":
      paths = <path d="M4 7h16M9 3.5h6L16 7H8zM6.5 7l1 14h9l1-14M10 11v6m4-6v6" />;
      break;
    case "up":
      paths = <><path d="m5 11 7-7 7 7" /><path d="M12 4v16" /></>;
      break;
    case "lock":
      paths = <><rect x="5" y="10" width="14" height="11" rx="2" /><path d="M8 10V7a4 4 0 0 1 8 0v3" /></>;
      break;
    case "layers":
      paths = <><path d="m12 3 8.5 4.5L12 12 3.5 7.5 12 3Z" /><path d="m4 12 8 4.2 8-4.2m-16 4.8 8 4.2 8-4.2" /></>;
      break;
    case "comment":
      paths = <path d="M4 4h16v13H9l-5 4z" />;
      break;
    case "folder":
      paths = <path d="M3 19.5V5h7l2 2h9v12.5Z" />;
      break;
    case "file":
      paths = <><path d="M6 2.5h8l4 4V21H6zM14 2.5v4h4" /><path d="M9 11h6m-6 4h6" /></>;
      break;
  }

  return (
    <svg
      className={["ui-icon", className].filter(Boolean).join(" ")}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...props}
    >
      {paths}
    </svg>
  );
}

// 系统真实图标缓存：按文件类型 key（扩展名或目录）缓存 data URL；null = 取失败。
const iconCache = new Map<string, string | null>();
const iconInflight = new Set<string>();
const iconSubs = new Set<() => void>();
const DIR_KEY = "\0dir";

function iconKey(name: string, isDir: boolean): string {
  if (isDir) return DIR_KEY;
  return name.split(".").pop()?.toLowerCase() ?? "";
}

function ensureIcon(key: string) {
  if (iconCache.has(key) || iconInflight.has(key)) return;
  iconInflight.add(key);
  const isDir = key === DIR_KEY;
  api
    .systemIcon(isDir ? "" : key, isDir)
    .then((d) => iconCache.set(key, d ? `data:image/png;base64,${d}` : null))
    .catch(() => iconCache.set(key, null))
    .finally(() => {
      iconInflight.delete(key);
      iconSubs.forEach((f) => f());
    });
}

/** 列表用文件图标：优先系统真实图标，未就绪时回退单色文件/文件夹图标。 */
export function FileIcon({ name, isDir }: { name: string; isDir: boolean }) {
  const key = iconKey(name, isDir);
  const [, force] = useState(0);
  useEffect(() => {
    const cb = () => force((n) => n + 1);
    iconSubs.add(cb);
    ensureIcon(key);
    return () => {
      iconSubs.delete(cb);
    };
  }, [key]);
  const url = iconCache.get(key);
  if (url) return <img className="ficon" src={url} alt="" draggable={false} />;
  return (
    <span className="icon">
      <UiIcon className="file-fallback" name={isDir ? "folder" : "file"} size={18} />
    </span>
  );
}
