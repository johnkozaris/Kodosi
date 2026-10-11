import type { KcEnvName, ThemeName } from "../kc.gen";

/** What Keycloak gives the account page: the data of its template `index.ftl`. */
export type KcContext = {
  themeType: "account";
  themeName: ThemeName;
  properties: Record<KcEnvName, string>;
  /** The address of Keycloak. */
  authServerUrl: string;
  /** Keycloak's own client for the account pages. */
  clientId: string;
  /** The language of the page, as a tag: "en". */
  locale: string;
  /** The languages of the realm: the tag, and Keycloak's name of the language. */
  supportedLocales?: Record<string, string>;
  realm: {
    name: string;
    registrationEmailAsUsername: boolean;
    editUsernameAllowed?: boolean;
    isInternationalizationEnabled: boolean;
  };
  /** Where the account page is. */
  baseUrl: { path: string };
  /** The program that the person came from, when its link names it. */
  referrerName?: string;
  referrer_uri?: string;
  isLinkedAccountsEnabled: boolean;
  isViewApplicationsEnabled?: boolean;
  deleteAccountAllowed: boolean;
  /** Keycloak's own words for an account, in the language of the page, as JSON. */
  msgJSON?: string;
};
