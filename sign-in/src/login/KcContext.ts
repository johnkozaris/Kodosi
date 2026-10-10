import type { ExtendKcContext } from "keycloakify/login";
import type { KcEnvName, ThemeName } from "../kc.gen";

type KcContextExtension = {
  themeName: ThemeName;
  properties: Record<KcEnvName, string>;
};

/**
 * The pages of a Turnstile step that come before a form, when the realm puts the check on a page
 * of its own (github.com/zymlabs/keycloak-cloudflare-turnstile-provider). The theme has them, so
 * they show in the frame of Kodosi.
 */
type KcContextExtensionPerPage = {
  "turnstile-form.ftl": {
    turnstileSiteKey: string;
    isResetFlow?: boolean;
    isRegistrationFlow?: boolean;
  };
  "turnstile-registration-form.ftl": { turnstileSiteKey: string };
};

export type KcContext = ExtendKcContext<KcContextExtension, KcContextExtensionPerPage>;

export type PageOf<Id extends KcContext["pageId"]> = Extract<KcContext, { pageId: Id }>;
