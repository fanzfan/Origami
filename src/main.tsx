import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { MiniProgress } from "./components/MiniProgress";
import "./styles.css";

const isMini = new URLSearchParams(window.location.search).has("mini");

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {isMini ? <MiniProgress /> : <App />}
  </React.StrictMode>,
);
