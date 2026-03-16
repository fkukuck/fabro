import { defineConfig } from "astro/config";
import react from "@astrojs/react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  integrations: [react()],
  redirects: {
    "/discord": {
      status: 302,
      destination:
        "https://discord.com/channels/1256822430505373696/1481682824196128822",
    },
    "/docs": {
      status: 302,
      destination: "https://docs.fabro.sh",
    },
  },
  vite: {
    plugins: [tailwindcss()],
  },
});
