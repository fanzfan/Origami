// 界面字体 / 缩放设置：持久化到 localStorage，并通过 document zoom + CSS 变量实时应用。

export interface Settings {
  scale: number; // 界面整体缩放（字体大小），1 = 100%
  font: string; // 字体族 key，见 FONTS
}

export const FONTS: [string, string, string][] = [
  // [key, 显示名, CSS font-family]
  ["system", "系统默认", `-apple-system, "SF Pro Text", "PingFang SC", "Microsoft YaHei", "Helvetica Neue", sans-serif`],
  ["sans", "无衬线", `"Helvetica Neue", Arial, "PingFang SC", "Microsoft YaHei", sans-serif`],
  ["serif", "衬线", `Georgia, "Songti SC", "SimSun", "Times New Roman", serif`],
  ["mono", "等宽", `"SF Mono", "JetBrains Mono", Menlo, Consolas, "Courier New", monospace`],
];

export const SCALE_MIN = 0.7;
export const SCALE_MAX = 1.8;
export const SCALE_STEP = 0.1;

const DEFAULTS: Settings = { scale: 1, font: "system" };

export function clampScale(s: number): number {
  return Math.min(SCALE_MAX, Math.max(SCALE_MIN, Math.round(s * 10) / 10));
}

export function loadSettings(): Settings {
  try {
    const raw = JSON.parse(localStorage.getItem("settings") ?? "{}");
    return {
      scale: typeof raw.scale === "number" ? clampScale(raw.scale) : DEFAULTS.scale,
      font: FONTS.some((f) => f[0] === raw.font) ? raw.font : DEFAULTS.font,
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
}
