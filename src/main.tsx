// The UI's entry point: mounts the React app into index.html's <div id="root">.
// React quick start: https://react.dev/learn

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";

// In the built app, turn off the webview's own right-click menu. Its entries
// ("Back", "Reload", "Save Image As"...) are meant for web pages and are
// useless or broken in a desktop app. The app's own menus (ContextMenu.tsx)
// are not affected: they are ordinary elements, not the webview's menu.
//
// Two exceptions:
// - Development (`npm run app`) keeps the menu, because it holds "Inspect
//   Element", which opens the developer tools. `import.meta.env.PROD` is
//   set by Vite and is true only in `npm run app:build`.
//   Docs: https://vite.dev/guide/env-and-mode
// - Text boxes keep it, because there it offers cut, copy and paste.
if (import.meta.env.PROD) {
  window.addEventListener("contextmenu", (event) => {
    const target = event.target as HTMLElement;
    if (target.closest("input[type=text], input[type=search], textarea")) return;
    event.preventDefault();
  });
}

createRoot(document.getElementById("root")!).render(
  // StrictMode runs extra checks in development (it deliberately mounts
  // components twice to flush out bugs). It does nothing in the built app.
  <StrictMode>
    <App />
  </StrictMode>,
);
