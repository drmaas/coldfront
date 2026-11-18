import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { applyResolvedTheme, readThemePreference, resolveTheme } from "./theme";
import "./styles.css";

applyResolvedTheme(resolveTheme(readThemePreference()));

const root = document.querySelector("#root");
if (!root) throw new Error("missing root");
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
