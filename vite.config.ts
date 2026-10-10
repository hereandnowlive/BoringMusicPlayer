// Vite builds the TypeScript UI. In development (`npm run app`) it also
// serves it with live reload; Tauri's window loads that dev server.
// Docs: https://vite.dev/config/
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  // The Tailwind plugin reads the source files, finds the utility classes
  // they use and generates the CSS for exactly those (see src/styles.css).
  // Docs: https://tailwindcss.com/docs/installation/using-vite
  plugins: [react(), tailwindcss()],

  // Keep Rust compiler output visible instead of clearing the terminal.
  clearScreen: false,

  server: {
    // Must match "devUrl" in src-tauri/tauri.conf.json.
    port: 1420,
    strictPort: true,
    watch: {
      // The Rust side is rebuilt by Tauri, not Vite.
      ignored: ["**/src-tauri/**"],
    },
  },
});
