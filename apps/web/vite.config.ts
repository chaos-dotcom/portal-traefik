import { sveltekit } from "@sveltejs/kit/vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  server: {
    // The app calls a same-origin /api; in `npm run dev`, forward it to a running Portal (by
    // default the container `make dev` starts on its loopback port).
    proxy: {
      "/api": process.env.PORTAL_API_URL ?? "http://127.0.0.1:18080",
    },
  },
});
