import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { keycloakify } from "keycloakify/vite-plugin";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    keycloakify({
      themeName: "kodosi",
      accountThemeImplementation: "Single-Page",
      // The terms and the privacy page of the service, at the foot of each page. The Keycloak
      // server sets them; without them, the foot has no links.
      environmentVariables: [
        { name: "KODOSI_TERMS_URL", default: "" },
        { name: "KODOSI_PRIVACY_URL", default: "" },
      ],
    }),
  ],
  server: { port: Number(process.env.KODOSI_SIGN_IN_PORT ?? 3184) },
});
