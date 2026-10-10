import "./styles.css";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import KcPage from "./login/KcPage";

const root = createRoot(document.getElementById("root") as HTMLElement);
const kcContext = window.kcContext;

if (kcContext?.themeType === "login") {
  root.render(
    <StrictMode>
      <KcPage kcContext={kcContext} />
    </StrictMode>,
  );
} else if (kcContext) {
  // The account page loads its own code: a sign-in does not carry it. When the code does not
  // come, the page loads one more time, and not in a circle.
  const AGAIN = "kodosi.again";
  void import("./account/KcPage").then(
    ({ default: AccountPage }) => {
      try {
        sessionStorage.removeItem(AGAIN);
      } catch {
        // A browser without storage.
      }
      root.render(
        <StrictMode>
          <AccountPage kcContext={kcContext} />
        </StrictMode>,
      );
    },
    () => {
      try {
        if (sessionStorage.getItem(AGAIN)) return;
        sessionStorage.setItem(AGAIN, "1");
        location.reload();
      } catch {
        // A browser without storage keeps the page as it is.
      }
    },
  );
} else if (import.meta.env.DEV) {
  // With no Keycloak behind the page, the development server shows every page with sample data.
  void import("./dev/gallery").then(({ Gallery }) => root.render(<Gallery />));
}
