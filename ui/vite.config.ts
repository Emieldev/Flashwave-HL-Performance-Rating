import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri drives this dev server, so the port is fixed and failures must be loud
// rather than silently falling back to another port the window won't load.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5174,
    strictPort: true,
    // The translations live in lang/ at the top of the repository, outside
    // this folder, so contributors find them without digging into the UI.
    fs: { allow: [".."] },
    watch: {
      // Rust rebuilds are handled by Tauri; watching them just churns HMR.
      ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"],
    },
  },
});
