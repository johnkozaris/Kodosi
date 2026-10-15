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
  const AGAIN = "kodosi.again";
  void import("./account/KcPage").then(
    ({ default: AccountPage }) => {
      try {
        sessionStorage.removeItem(AGAIN);
      } catch {}
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
      } catch {}
    },
  );
} else if (import.meta.env.DEV) {
  void import("./dev/gallery").then(({ Gallery }) => root.render(<Gallery />));
}
