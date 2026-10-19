import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { MiniProgress } from "./components/MiniProgress";
import { AskDialog } from "./components/AskDialog";
import { applySettings, loadSettings, readThemeRgb } from "./settings";
import "./styles.css";

const params = new URLSearchParams(window.location.search);
const isMini = params.has("mini");
const isAsk = params.has("ask");
const platform = /Windows/.test(navigator.userAgent) || navigator.platform.startsWith("Win")
  ? "windows"
  : /Mac|iPhone|iPad/.test(navigator.platform) || /Mac OS X/.test(navigator.userAgent)
    ? "macos"
    : "other";

document.documentElement.setAttribute("data-platform", platform);

// 启动即应用外观（深浅/主题/字体/缩放）。主窗 App 之后还会在状态变化时再调用，
// 但迷你窗 / ask 小窗只渲染各自组件、不走 App，故必须在此统一应用一次。
const startupSettings = loadSettings();
applySettings(startupSettings);

// 只有主窗要毛玻璃材质（data-effects=on 让 body 透明透出材质）。
// 迷你窗、ask 小窗都不透明、无材质：ask 小窗就是应用内那个对话框卡片，纯色背景即可。
if (!isMini && !isAsk) {
  document.documentElement.setAttribute("data-effects", startupSettings.material !== "none" ? "on" : "off");
}
// ask 小窗：去掉模态的深色遮罩（standalone 窗口不需要压暗背景），卡片直接浮在应用背景色上。
if (isMini) document.documentElement.setAttribute("data-window", "mini");
else if (isAsk) document.documentElement.setAttribute("data-window", "ask");
else document.documentElement.setAttribute("data-window", "main");

// Windows 的任务小窗：卡片铺满整窗，窗口边缘那圈 Win11 系统描边染成卡片底色即隐形，
// 同时保住系统圆角（描边直接去掉会连圆角一起丢，详见 winmat::apply_rounded_frame）。
// 须在 applySettings 之后取色，此时 data-mode / data-theme 已就位。
if ((isAsk || isMini) && platform === "windows") {
  const [r, g, b] = readThemeRgb("--bg-elev");
  import("./api").then(({ api }) => api.setWindowBorderColor(r, g, b)).catch(() => {});
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {isMini ? <MiniProgress /> : isAsk ? <AskDialog /> : <App />}
  </React.StrictMode>,
);
