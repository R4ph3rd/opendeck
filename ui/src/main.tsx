import "@opendeck/design-system/styles.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { installGlobalErrorHandlers } from "./lib/diagnostics";

const root = document.getElementById("root");
if (!root) {
  throw new Error("missing #root element");
}

installGlobalErrorHandlers();

createRoot(root).render(
  <StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </StrictMode>,
);
