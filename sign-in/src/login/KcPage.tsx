import { lazy, Suspense } from "react";

import { useI18n } from "./i18n";
import type { KcContext } from "./KcContext";
import { CheckPage } from "./pages/check";
import { LinkAccount, LinkByEmail, Ways } from "./pages/choices";
import { ConfigTotp, Otp, RecoveryConfig, RecoveryInput } from "./pages/codes";
import { DeviceCode, Grant } from "./pages/device";
import { NewEmail, Register, ReviewProfile, Welcome } from "./pages/join";
import { DeleteAccount, Expired, Failure, Info, RemoveWay, SignOut } from "./pages/notices";
import { NewPasskey, Passkey, PasskeyAtName, PasskeyFailed } from "./pages/passkey";
import { ResetPassword, UpdatePassword, VerifyEmail } from "./pages/password";
import { Login, LoginPassword, LoginUsername } from "./pages/sign-in";

// The rare pages keep Keycloak's own form code. It loads when a person meets one of them.
const Other = lazy(() => import("./pages/other"));
// The terms are the realm's own text, with its markup: the page brings the code that cleans it.
const Terms = lazy(() => import("./pages/terms"));

export default function KcPage({ kcContext }: { kcContext: KcContext }) {
  const { i18n } = useI18n({ kcContext });
  switch (kcContext.pageId) {
    case "login.ftl":
      return <Login kcContext={kcContext} i18n={i18n} />;
    case "login-username.ftl":
      return <LoginUsername kcContext={kcContext} i18n={i18n} />;
    case "login-password.ftl":
      return <LoginPassword kcContext={kcContext} i18n={i18n} />;
    case "login-oauth2-device-verify-user-code.ftl":
      return <DeviceCode kcContext={kcContext} i18n={i18n} />;
    case "login-oauth-grant.ftl":
      return <Grant kcContext={kcContext} i18n={i18n} />;
    case "register.ftl":
      return <Register kcContext={kcContext} i18n={i18n} />;
    case "login-update-profile.ftl":
      return <Welcome kcContext={kcContext} i18n={i18n} />;
    case "idp-review-user-profile.ftl":
      return <ReviewProfile kcContext={kcContext} i18n={i18n} />;
    case "update-email.ftl":
      return <NewEmail kcContext={kcContext} i18n={i18n} />;
    case "login-reset-password.ftl":
      return <ResetPassword kcContext={kcContext} i18n={i18n} />;
    case "login-update-password.ftl":
      return <UpdatePassword kcContext={kcContext} i18n={i18n} />;
    case "login-verify-email.ftl":
      return <VerifyEmail kcContext={kcContext} i18n={i18n} />;
    case "login-otp.ftl":
      return <Otp kcContext={kcContext} i18n={i18n} />;
    case "login-config-totp.ftl":
      return <ConfigTotp kcContext={kcContext} i18n={i18n} />;
    case "login-recovery-authn-code-input.ftl":
      return <RecoveryInput kcContext={kcContext} i18n={i18n} />;
    case "login-recovery-authn-code-config.ftl":
      return <RecoveryConfig kcContext={kcContext} i18n={i18n} />;
    case "webauthn-authenticate.ftl":
      return <Passkey kcContext={kcContext} i18n={i18n} />;
    case "login-passkeys-conditional-authenticate.ftl":
      return <PasskeyAtName kcContext={kcContext} i18n={i18n} />;
    case "webauthn-register.ftl":
      return <NewPasskey kcContext={kcContext} i18n={i18n} />;
    case "webauthn-error.ftl":
      return <PasskeyFailed kcContext={kcContext} i18n={i18n} />;
    case "select-authenticator.ftl":
      return <Ways kcContext={kcContext} i18n={i18n} />;
    case "login-idp-link-confirm.ftl":
      return <LinkAccount kcContext={kcContext} i18n={i18n} />;
    case "login-idp-link-email.ftl":
      return <LinkByEmail kcContext={kcContext} i18n={i18n} />;
    case "info.ftl":
      return <Info kcContext={kcContext} i18n={i18n} />;
    case "error.ftl":
      return <Failure kcContext={kcContext} i18n={i18n} />;
    case "login-page-expired.ftl":
      return <Expired kcContext={kcContext} i18n={i18n} />;
    case "logout-confirm.ftl":
      return <SignOut kcContext={kcContext} i18n={i18n} />;
    case "delete-credential.ftl":
      return <RemoveWay kcContext={kcContext} i18n={i18n} />;
    case "delete-account-confirm.ftl":
      return <DeleteAccount kcContext={kcContext} i18n={i18n} />;
    case "turnstile-form.ftl":
    case "turnstile-registration-form.ftl":
      return <CheckPage kcContext={kcContext} i18n={i18n} />;
    case "terms.ftl":
      return (
        <Suspense>
          <Terms kcContext={kcContext} i18n={i18n} />
        </Suspense>
      );
    default:
      return (
        <Suspense>
          <Other kcContext={kcContext} i18n={i18n} />
        </Suspense>
      );
  }
}
