import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { keycloakify } from "keycloakify/vite-plugin";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    keycloakify({ themeName: "kodosi", accountThemeImplementation: "Single-Page" }),
  ],
  server: { port: Number(process.env.KODOSI_SIGN_IN_PORT ?? 3184) },
});
