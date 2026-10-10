import type { KcContext } from "../account/KcContext";
import {
  type AccountService,
  type Person,
  Refused,
  type Session,
  Unreachable,
  type Way,
} from "../account/service";
import { kcEnvDefaults } from "../kc.gen";

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

/** What Keycloak gives the account page, for the development server. */
export function sampleContext(): KcContext {
  return {
    themeType: "account",
    themeName: "kodosi",
    properties: { ...kcEnvDefaults },
    authServerUrl: "/",
    clientId: "account-console",
    locale: "en",
    realm: {
      name: "kodosi",
      registrationEmailAsUsername: false,
      isInternationalizationEnabled: false,
    },
    baseUrl: { path: "/" },
    isLinkedAccountsEnabled: true,
    deleteAccountAllowed: true,
  };
}

/** The scenes of the sign-in pages that a step of the account page goes to. */
const STEPS: Record<string, string> = {
  UPDATE_PASSWORD: "new-password",
  CONFIGURE_TOTP: "authenticator",
  CONFIGURE_RECOVERY_AUTHN_CODES: "recovery-list",
  "webauthn-register-passwordless": "passkey-new",
  UPDATE_EMAIL: "new-email",
  delete_account: "delete",
};

/**
 * An account service with no Keycloak behind it. `kind` says what the person has: "full" is a
 * person with each way of sign-in, and "new" has a password only. With "down", the service fails.
 */
export function sampleAccount(kind: string, to: (scene: string) => string): AccountService {
  const wait = (ms = 320) => new Promise<void>((done) => window.setTimeout(done, ms));
  const full = kind !== "new";
  const person: Person = {
    username: "maya",
    email: "maya@kodosi.example",
    emailStep: true,
    fields: [
      { name: "username", label: "${username}", value: "maya", required: true, readOnly: true },
      {
        name: "email",
        label: "${email}",
        value: "maya@kodosi.example",
        required: true,
        readOnly: true,
      },
      { name: "firstName", label: "${firstName}", value: "Maya", required: true, readOnly: false },
      { name: "lastName", label: "${lastName}", value: "Chen", required: true, readOnly: false },
    ],
  };
  const ways: Way[] = [
    {
      type: "password",
      update: "UPDATE_PASSWORD",
      removable: false,
      held: [{ id: "p", created: Date.now() - 41 * DAY }],
    },
    {
      type: "otp",
      create: "CONFIGURE_TOTP",
      removable: true,
      held: full ? [{ id: "o", label: "iPhone", created: Date.now() - 40 * DAY }] : [],
    },
    {
      type: "webauthn-passwordless",
      create: "webauthn-register-passwordless",
      removable: true,
      held: full ? [{ id: "k", label: "MacBook Pro", created: Date.now() - 12 * DAY }] : [],
    },
    {
      type: "recovery-authn-codes",
      create: "CONFIGURE_RECOVERY_AUTHN_CODES",
      removable: true,
      held: full ? [{ id: "r", created: Date.now() - 40 * DAY, left: 9, total: 12 }] : [],
    },
  ];
  let sessions: Session[] = [
    {
      id: "a",
      browser: "Chrome",
      os: "Mac OS X",
      mobile: false,
      current: true,
      lastAccess: Date.now(),
      programs: ["Kodosi"],
    },
    ...(full
      ? [
          {
            id: "b",
            browser: "Firefox",
            os: "Linux",
            mobile: false,
            current: false,
            lastAccess: Date.now() - 3 * HOUR,
            programs: ["Kodosi"],
          },
          {
            id: "c",
            browser: "Safari",
            os: "iOS",
            mobile: true,
            current: false,
            lastAccess: Date.now() - 2 * DAY,
            programs: [],
          },
        ]
      : []),
  ];
  const came = new URLSearchParams(location.search).get("came");

  return {
    came: came ? { action: "UPDATE_PASSWORD", status: came as "success" } : null,
    async person() {
      await wait();
      if (kind === "down") throw new Unreachable();
      return person;
    },
    async save(values) {
      await wait();
      const empty = Object.entries(values).find(([, value]) => !value);
      if (empty)
        throw new Refused(
          [{ key: "error-user-attribute-required", params: [empty[0]] }],
          [empty[0]],
        );
      for (const field of person.fields) {
        const next = values[field.name];
        if (next !== undefined) field.value = next;
      }
    },
    async ways() {
      await wait();
      return ways;
    },
    async sessions() {
      await wait();
      return sessions;
    },
    async endSession(id) {
      await wait();
      sessions = sessions.filter((session) => session.id !== id);
    },
    async endOtherSessions() {
      await wait();
      sessions = sessions.filter((session) => session.current);
    },
    async linked() {
      return [
        full
          ? { alias: "github", name: "GitHub", providerId: "github", connected: true, as: "maya" }
          : { alias: "github", name: "GitHub", providerId: "github", connected: false },
        { alias: "google", name: "Google", providerId: "google", connected: false },
        { alias: "apple", name: "Apple", providerId: "oidc", connected: false },
      ];
    },
    async unlink() {
      await wait();
    },
    async programs() {
      return [{ clientId: "kodosi-app", name: "Kodosi", stays: true }];
    },
    async revoke() {
      await wait();
    },
    async setLanguage() {
      await wait();
    },
    act(action) {
      location.assign(
        to(action.startsWith("delete_credential") ? "remove" : (STEPS[action] ?? "info")),
      );
    },
    signOut() {
      location.assign(to("sign-in-plain"));
    },
  };
}
