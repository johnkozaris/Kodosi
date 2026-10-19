import type { ExtendKcContext } from "keycloakify/login";
import type { KcEnvName, ThemeName } from "../kc.gen";

type KcContextExtension = {
  themeName: ThemeName;
  properties: Record<KcEnvName, string>;
};

type KcContextExtensionPerPage = {
  "turnstile-form.ftl": {
    turnstileSiteKey: string;
    isResetFlow?: boolean;
    isRegistrationFlow?: boolean;
  };
  "turnstile-registration-form.ftl": { turnstileSiteKey: string };
  "login-idp-link-confirm-override.ftl": { brokerContext?: { username: string } };
};

export type KcContext = ExtendKcContext<KcContextExtension, KcContextExtensionPerPage>;
