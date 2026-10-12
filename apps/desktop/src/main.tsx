import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.js";
import "../../../packages/design-tokens/theme.css";
import "./styles.css";

const root = document.getElementById("root");

if (root === null) {
  throw new Error("The desktop host did not provide a root element.");
}

async function start(container: HTMLElement) {
  // 端到端测试模式：把 invoke 接到真实内核的测试桥接（正式构建会剔除这一分支）。
  if (import.meta.env.MODE === "e2e") {
    (await import("./e2e/installBridge.js")).installBridge();
  }
  createRoot(container).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

void start(root);
