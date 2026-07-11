// 应用设置：持久化到 localStorage。
// - 外观：界面缩放（document zoom）、字体族、主题（CSS 变量 / data-theme）。
// - 压缩：默认压缩等级、是否剔除系统垃圾文件。

// 外观模式：跟随系统 / 强制浅色 / 强制深色。
export type AppearanceMode = "system" | "light" | "dark";

// 窗口材质：亚克力（Acrylic）/ 云母（Mica）/ 无（普通不透明）。
// Windows 11 三者皆可选；macOS 上 acrylic/mica 统一表现为毛玻璃，none 为无材质。
export type WindowMaterial = "acrylic" | "mica" | "none";

export interface Settings {
  scale: number; // 界面整体缩放（字体大小），1 = 100%
  font: string; // 字体族 key，见 FONTS
  theme: string; // 主题 key，见 THEMES
  mode: AppearanceMode; // 外观模式（亮/暗/跟随系统）
  material: WindowMaterial; // 窗口材质（Win11 亚克力/云母/无；macOS 毛玻璃）
  acrylicOpacity: number; // 亚克力色调不透明度 0..100（仅 Windows 亚克力生效；越小越透）
  level: number; // 默认压缩等级 0(仅存储)..9(最高)
  excludeJunk: boolean; // 压缩时剔除 .DS_Store / __MACOSX / Thumbs.db 等
  openAfterExtract: boolean; // 解压完成后打开目标目录
}

// [key, 显示名]。深浅配色在 styles.css 里按 data-mode 与 prefers-color-scheme 切换。
export const MODES: [AppearanceMode, string][] = [
  ["system", "跟随系统"],
  ["light", "浅色"],
  ["dark", "深色"],
];

// [key, 显示名]。Windows 设置里以下拉菜单呈现；顺序即菜单顺序。
export const MATERIALS: [WindowMaterial, string][] = [
  ["acrylic", "亚克力"],
  ["mica", "云母（Mica）"],
  ["none", "无（普通）"],
];

export const FONTS: [string, string, string][] = [
  // [key, 显示名, CSS font-family]
  ["system", "系统默认", `-apple-system, "SF Pro Text", "PingFang SC", "Microsoft YaHei", "Helvetica Neue", sans-serif`],
  ["sans", "无衬线", `"Helvetica Neue", Arial, "PingFang SC", "Microsoft YaHei", sans-serif`],
  ["serif", "衬线", `Georgia, "Songti SC", "SimSun", "Times New Roman", serif`],
  ["mono", "等宽", `"SF Mono", "JetBrains Mono", Menlo, Consolas, "Courier New", monospace`],
];

// [key, 显示名, 代表色(用于设置里的色块预览)]。实际配色在 styles.css 的
// :root[data-theme=...] 块中按浅色/深色分别定义。"default" 使用 Origami 品牌蓝紫。
export const THEMES: [string, string, string][] = [
  ["default", "折纸蓝", "#5b5fef"],
  ["ocean", "海洋蓝", "#0ea5e9"],
  ["forest", "森林绿", "#16a34a"],
  ["grape", "葡萄紫", "#8b5cf6"],
  ["rose", "玫瑰粉", "#ec4899"],
  ["slate", "石墨灰", "#64748b"],
];

export const SCALE_MIN = 0.7;
export const SCALE_MAX = 1.8;
export const SCALE_STEP = 0.1;

// 亚克力色调不透明度范围（%）。下限留出一点实感，避免过透导致文字不可读；
// 上限不到 100，保留一丝通透感（100% 就跟无材质的纯色差不多了）。
export const ACRYLIC_OPACITY_MIN = 10;
export const ACRYLIC_OPACITY_MAX = 95;

const DEFAULTS: Settings = { scale: 1, font: "system", theme: "default", mode: "system", material: "acrylic", acrylicOpacity: 90, level: 6, excludeJunk: true, openAfterExtract: true };

export function clampScale(s: number): number {
  return Math.min(SCALE_MAX, Math.max(SCALE_MIN, Math.round(s * 10) / 10));
}

export function clampAcrylicOpacity(n: number): number {
  return Math.min(ACRYLIC_OPACITY_MAX, Math.max(ACRYLIC_OPACITY_MIN, Math.round(n)));
}

function clampLevel(n: number): number {
  return Math.min(9, Math.max(0, Math.round(n)));
}

// 读取窗口材质，兼容旧版布尔 `effects`：true→默认亚克力，false→无。
function readMaterial(raw: any): WindowMaterial {
  if (MATERIALS.some((m) => m[0] === raw.material)) return raw.material;
  if (typeof raw.effects === "boolean") return raw.effects ? DEFAULTS.material : "none";
  return DEFAULTS.material;
}

export function loadSettings(): Settings {
  try {
    const raw = JSON.parse(localStorage.getItem("settings") ?? "{}");
    return {
      scale: typeof raw.scale === "number" ? clampScale(raw.scale) : DEFAULTS.scale,
      font: FONTS.some((f) => f[0] === raw.font) ? raw.font : DEFAULTS.font,
      theme: THEMES.some((t) => t[0] === raw.theme) ? raw.theme : DEFAULTS.theme,
      mode: MODES.some((m) => m[0] === raw.mode) ? raw.mode : DEFAULTS.mode,
      material: readMaterial(raw),
      acrylicOpacity:
        typeof raw.acrylicOpacity === "number" ? clampAcrylicOpacity(raw.acrylicOpacity) : DEFAULTS.acrylicOpacity,
      level: typeof raw.level === "number" ? clampLevel(raw.level) : DEFAULTS.level,
      excludeJunk: typeof raw.excludeJunk === "boolean" ? raw.excludeJunk : DEFAULTS.excludeJunk,
      openAfterExtract:
        typeof raw.openAfterExtract === "boolean" ? raw.openAfterExtract : DEFAULTS.openAfterExtract,
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
  // 外观模式：data-mode 驱动 CSS 的深浅切换；color-scheme 让原生控件(滚动条/下拉/
  // 复选框)随之变色。system 时交给 prefers-color-scheme 与 UA 自行决定。
  root.setAttribute("data-mode", s.mode);
  root.style.colorScheme = s.mode === "system" ? "light dark" : s.mode;
}

const isWindows = () => /Windows/.test(navigator.userAgent) || navigator.platform.startsWith("Win");
const isMacOS = () => /Mac|iPhone|iPad/.test(navigator.platform) || /Mac OS X/.test(navigator.userAgent);

// 取当前主题的窗口背景色（CSS 变量 --bg，形如 #rrggbb）作亚克力色调基色，
// 使玻璃色调与应用配色一致。解析失败时按深/浅色给个中性回退值。
function readBgRgb(): [number, number, number] {
  const raw = getComputedStyle(document.documentElement).getPropertyValue("--bg").trim();
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(raw);
  if (m) return [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)];
  const dark = matchMedia("(prefers-color-scheme: dark)").matches;
  return dark ? [22, 24, 29] : [245, 246, 248];
}

// 窗口材质（Win11 亚克力/云母 / macOS 毛玻璃）。仅作用于当前（主）窗口。
//
// 默认材质由 tauri.conf.json 的 windowEffects 在建窗时施加（最可靠）。这里负责：
//   1) data-effects 属性驱动 CSS（"on" 让 body 透明、材质透出；"off" 保持不透明）；
//   2) 运行时切换：无→清除材质，亚克力/云母→重新施加对应材质；
//   3) Windows 亚克力走自定义 accent 通道以支持可调透明度（见 winmat.rs）。
// 注意：Win11 材质还需系统「透明效果」开启，且材质感来自桌面壁纸（纯色桌面看不出）。
export async function applyWindowEffects(s: Settings) {
  const on = s.material !== "none";
  document.documentElement.setAttribute("data-effects", on ? "on" : "off");
  try {
    const { getCurrentWindow, Effect, EffectState } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();

    // Windows 亚克力：绕过 Tauri 的 setEffects（Win11 上不支持自定义透明度），
    // 改用自定义命令按 acrylicOpacity 施加可调透明度的亚克力。
    if (isWindows() && s.material === "acrylic") {
      const { api } = await import("./api");
      const [r, g, b] = readBgRgb();
      const alpha = Math.round((clampAcrylicOpacity(s.acrylicOpacity) / 100) * 255);
      await win.setEffects({ effects: [] }); // 清掉建窗时的默认亚克力，交给 accent 通道
      await api.setAcrylic(true, r, g, b, alpha);
      return;
    }

    // 其它情形先清除可能残留的 accent 亚克力，再走标准 setEffects。
    if (isWindows()) {
      const { api } = await import("./api");
      await api.setAcrylic(false, 0, 0, 0, 0);
    }

    if (!on) {
      await win.setEffects({ effects: [] }); // 清除材质，恢复普通不透明窗口
      return;
    }
    if (isMacOS()) {
      // macOS 无 Win11 材质之分，亚克力/云母统一走毛玻璃（UnderWindowBackground）。
      await win.setEffects({ effects: [Effect.UnderWindowBackground], state: EffectState.FollowsWindowActiveState });
      return;
    }
    // Windows 云母（Mica）：跟随窗口激活态。亚克力已在上面处理。
    const effect = s.material === "mica" ? Effect.Mica : Effect.Acrylic;
    await win.setEffects({ effects: [effect], state: EffectState.FollowsWindowActiveState });
  } catch {
    // 运行时施加失败不影响默认观感：材质已由配置在建窗时施加。
  }
}
