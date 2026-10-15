import type { KcEnvName, ThemeName } from "../kc.gen";

export type KcContext = {
  themeType: "account";
  themeName: ThemeName;
  properties: Record<KcEnvName, string>;
  authServerUrl: string;
  clientId: string;
  locale: string;
  supportedLocales?: Record<string, string>;
  realm: {
    name: string;
    registrationEmailAsUsername: boolean;
    isInternationalizationEnabled: boolean;
  };
  baseUrl: { path: string };
  referrerName?: string;
  referrer_uri?: string;
  isLinkedAccountsEnabled: boolean;
  isViewApplicationsEnabled?: boolean;
  deleteAccountAllowed: boolean;
  msgJSON?: string;
};
