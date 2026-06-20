import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { MiniProgress } from "./components/MiniProgress";
import { applySettings, loadSettings } from "./settings";
import "./styles.css";

const isMini = new URLSearchParams(window.location.search).has("mini");

// 启动即应用外观（深浅/主题/字体/缩放）。主窗 App 之后还会在状态变化时再调用，
// 但迷你窗只渲染 MiniProgress、不走 App，故必须在此统一应用一次。
const startupSettings = loadSettings();
applySettings(startupSettings);

// 主窗：首屏前预置 data-effects，避免不透明 body 遮挡建窗时施加的材质（Mica/毛玻璃）。
// 迷你窗不透明，保持纯色，不设此属性。
if (!isMini) {
  document.documentElement.setAttribute("data-effects", startupSettings.effects ? "on" : "off");
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {isMini ? <MiniProgress /> : <App />}
  </React.StrictMode>,
);
