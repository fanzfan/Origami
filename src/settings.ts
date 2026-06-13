// 应用设置：持久化到 localStorage。
// - 外观：界面缩放（document zoom）、字体族、主题（CSS 变量 / data-theme）。
// - 压缩：默认压缩等级、是否剔除系统垃圾文件。

export interface Settings {
  scale: number; // 界面整体缩放（字体大小），1 = 100%
  font: string; // 字体族 key，见 FONTS
  theme: string; // 主题 key，见 THEMES
  level: number; // 默认压缩等级 0(仅存储)..9(最高)
  excludeJunk: boolean; // 压缩时剔除 .DS_Store / __MACOSX / Thumbs.db 等
}

export const FONTS: [string, string, string][] = [
  // [key, 显示名, CSS font-family]
  ["system", "系统默认", `-apple-system, "SF Pro Text", "PingFang SC", "Microsoft YaHei", "Helvetica Neue", sans-serif`],
  ["sans", "无衬线", `"Helvetica Neue", Arial, "PingFang SC", "Microsoft YaHei", sans-serif`],
  ["serif", "衬线", `Georgia, "Songti SC", "SimSun", "Times New Roman", serif`],
  ["mono", "等宽", `"SF Mono", "JetBrains Mono", Menlo, Consolas, "Courier New", monospace`],
];

// [key, 显示名, 代表色(用于设置里的色块预览)]。实际配色在 styles.css 的
// :root[data-theme=...] 块中按浅色/深色分别定义。"default" 用内置橙色。
export const THEMES: [string, string, string][] = [
  ["default", "折纸橙", "#f97316"],
  ["ocean", "海洋蓝", "#0ea5e9"],
  ["forest", "森林绿", "#16a34a"],
  ["grape", "葡萄紫", "#8b5cf6"],
  ["rose", "玫瑰粉", "#ec4899"],
  ["slate", "石墨灰", "#64748b"],
];

export const SCALE_MIN = 0.7;
export const SCALE_MAX = 1.8;
export const SCALE_STEP = 0.1;

const DEFAULTS: Settings = { scale: 1, font: "system", theme: "default", level: 6, excludeJunk: true };

export function clampScale(s: number): number {
  return Math.min(SCALE_MAX, Math.max(SCALE_MIN, Math.round(s * 10) / 10));
}

function clampLevel(n: number): number {
  return Math.min(9, Math.max(0, Math.round(n)));
}

export function loadSettings(): Settings {
  try {
    const raw = JSON.parse(localStorage.getItem("settings") ?? "{}");
    return {
      scale: typeof raw.scale === "number" ? clampScale(raw.scale) : DEFAULTS.scale,
      font: FONTS.some((f) => f[0] === raw.font) ? raw.font : DEFAULTS.font,
      theme: THEMES.some((t) => t[0] === raw.theme) ? raw.theme : DEFAULTS.theme,
      level: typeof raw.level === "number" ? clampLevel(raw.level) : DEFAULTS.level,
      excludeJunk: typeof raw.excludeJunk === "boolean" ? raw.excludeJunk : DEFAULTS.excludeJunk,
    };
  } catch {
    return { ...DEFAULTS };
  }
}

export function saveSettings(s: Settings) {
  localStorage.setItem("settings", JSON.stringify(s));
}

export function applySettings(s: Settings) {
  const root = document.documentElement;
  // zoom 在 WebKit / WebView2 中均支持，可整体缩放含 px 布局的界面。
  root.style.setProperty("zoom", String(s.scale));
  const fam = FONTS.find((f) => f[0] === s.font)?.[2] ?? FONTS[0][2];
  root.style.setProperty("--font-family", fam);
  // 主题通过 data-theme 切换，配色在 CSS 里按浅/深色分别定义，避免覆盖暗色模式。
  if (s.theme && s.theme !== "default") {
    root.setAttribute("data-theme", s.theme);
  } else {
    root.removeAttribute("data-theme");
  }
}
