import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { keycloakify } from "keycloakify/vite-plugin";
import { defineConfig } from "vite";
import { CARRY_FRESH_MS, CARRY_KEY } from "./src/frame/carry-storage.ts";

export default defineConfig({
  plugins: [
    {
      name: "kodosi-carry-storage",
      transformIndexHtml: {
        order: "pre",
        handler: (html) =>
          html
            .replaceAll("%CARRY_KEY%", CARRY_KEY)
            .replaceAll("%CARRY_FRESH_MS%", String(CARRY_FRESH_MS)),
      },
    },
    react(),
    tailwindcss(),
    keycloakify({
      themeName: "kodosi",
      accountThemeImplementation: "Single-Page",
      environmentVariables: [
        { name: "KODOSI_TERMS_URL", default: "" },
        { name: "KODOSI_PRIVACY_URL", default: "" },
      ],
    }),
  ],
  server: { port: Number(process.env.KODOSI_SIGN_IN_PORT ?? 3184) },
});
