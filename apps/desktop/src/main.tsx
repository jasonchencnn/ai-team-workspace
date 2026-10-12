import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.js";
import "../../../packages/design-tokens/theme.css";
import "./styles.css";

const root = document.getElementById("root");

if (root === null) {
  throw new Error("The desktop host did not provide a root element.");
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
