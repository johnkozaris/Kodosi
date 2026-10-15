import { createGetKcContextMock } from "keycloakify/login/KcContext";
import { useEffect } from "react";

import { Account } from "../account/account";
import type { AccountService } from "../account/service";
import { themeNames } from "../kc.gen";
import { dropCode, keepCode } from "../login/device";
import type { KcContext } from "../login/KcContext";
import KcPage from "../login/KcPage";
import { sampleAccount, sampleContext, sampleProperties } from "./account-sample";

const { getKcContextMock } = createGetKcContextMock({
  kcContextExtension: { themeName: themeNames[0], properties: sampleProperties },
  kcContextExtensionPerPage: {
    "turnstile-form.ftl": { turnstileSiteKey: "1x00000000000000000000AA" },
    "turnstile-registration-form.ftl": { turnstileSiteKey: "1x00000000000000000000AA" },
  },
  overrides: {},
  overridesPerPage: {},
});

type Overrides = Record<string, unknown>;
interface Scene {
  page: KcContext["pageId"];
  with?: Overrides;
  next?: string;
  code?: boolean;
  tail?: string;
  attributes?: Record<string, unknown>;
  account?: string;
}

const refused = (field: string, text: string) => ({
  messagesPerField: {
    existsError: (...fields: string[]) => fields.includes(field),
    exists: (name: string) => name === field,
    get: (name: string) => (name === field ? text : ""),
    getFirstError: () => text,
    printIfExists: () => undefined,
  },
});

const CODE = "WDJB-MJHT";
const person = { showUsername: true, attemptedUsername: "maya" };
const providers = [
  { alias: "github", loginUrl: "#", displayName: "GitHub", providerId: "github" },
  { alias: "google", loginUrl: "#", displayName: "Google", providerId: "google" },
  { alias: "apple", loginUrl: "#", displayName: "Apple", providerId: "oidc" },
];
const open = {
  realm: { registrationAllowed: true, resetPasswordAllowed: true, rememberMe: false },
  social: { providers },
};
const kodosiApp = {
  clientId: "kodosi-app",
  name: "Kodosi",
  attributes: { "oauth2.device.authorization.grant.enabled": "true" },
};
const scopes = {
  oauth: {
    code: "sample",
    client: "kodosi-app",
    clientScopesRequested: [
      { consentScreenText: "${profileScopeConsentText}" },
      { consentScreenText: "${offlineAccessScopeConsentText}" },
    ],
  },
  client: kodosiApp,
};
const attribute = (name: string, more: Overrides = {}) => ({
  name,
  displayName: `\${${name}}`,
  required: true,
  readOnly: false,
  validators: {},
  annotations: {},
  ...more,
});
const profile = (values: Record<string, string> = {}) => ({
  username: attribute("username", {
    value: values.username,
    autocomplete: "username",
    validators: {
      length: { min: "3", max: "40" },
      pattern: {
        pattern: "^[a-z0-9_-]+$",
        "error-message": "Use 3 to 40 lowercase letters, digits, hyphens or underscores.",
      },
    },
  }),
  email: attribute("email", {
    value: values.email,
    autocomplete: "email",
    validators: { email: {} },
  }),
  firstName: attribute("firstName", {
    value: values.firstName,
    autocomplete: "given-name",
    required: false,
  }),
  lastName: attribute("lastName", {
    value: values.lastName,
    autocomplete: "family-name",
    required: false,
  }),
});
const maya = {
  username: "maya",
  firstName: "Maya",
  lastName: "Chen",
  email: "maya@kodosi.example",
};
const terms =
  '<a href="https://kodosi.com/terms">Terms</a> · <a href="https://kodosi.com/privacy">Privacy</a>';
const PASSES = "1x00000000000000000000AA";
const BLOCKS = "2x00000000000000000000AB";
const ASKS = "3x00000000000000000000FF";
const turnstile = (key: string) => ({
  turnstileRequired: true,
  turnstileSiteKey: key,
  turnstileMode: "managed",
  turnstileTheme: "auto",
});
const injected = (key: string) => ({
  scripts: [
    `/realms/kodosi/kodosi-turnstile/config.js?siteKey=${key}&mode=managed&theme=auto&debug=false`,
    "/realms/kodosi/kodosi-turnstile/resources/js/turnstile-injector.js",
  ],
});
const checkRefused = {
  message: { type: "error", summary: "The check did not pass. Try it again." },
};
const oneApp = { otpLogin: { userOtpCredentials: [] } };

const SCENES: Record<string, Scene> = {
  "sign-in": { page: "login.ftl", with: { ...open, client: kodosiApp }, code: true, next: "check" },
  "sign-in-plain": { page: "login.ftl", next: "sign-in-wrong" },
  "sign-in-wrong": {
    page: "login.ftl",
    with: {
      ...open,
      login: { username: "maya" },
      ...refused("username", "That name or password is not right."),
    },
    next: "code",
  },
  "sign-in-passkey": { page: "login.ftl", with: { ...open, enableWebAuthnConditionalUI: true } },
  "sign-in-passkey-failed": {
    page: "login.ftl",
    with: {
      ...open,
      enableWebAuthnConditionalUI: true,
      message: {
        type: "error",
        summary: "The passkey did not answer. Try again, or use your password.",
      },
    },
  },
  name: { page: "login-username.ftl", with: open, next: "password" },
  password: { page: "login-password.ftl", with: { auth: person }, next: "code" },
  "password-wrong": {
    page: "login-password.ftl",
    with: { auth: person, ...refused("password", "That name or password is not right.") },
    next: "code",
  },

  device: { page: "login-oauth2-device-verify-user-code.ftl", next: "sign-in" },
  "device-app": {
    page: "login-oauth2-device-verify-user-code.ftl",
    tail: `#${CODE}`,
    next: "sign-in",
  },
  "device-wrong": {
    page: "login-oauth2-device-verify-user-code.ftl",
    with: {
      message: { type: "error", summary: "That code is not right. Check the code in Kodosi." },
    },
    next: "sign-in",
  },
  check: { page: "login-oauth-grant.ftl", with: scopes, code: true, next: "done" },
  connect: { page: "login-oauth-grant.ftl", with: scopes, next: "done" },
  grant: {
    page: "login-oauth-grant.ftl",
    with: { ...scopes, client: { clientId: "calendar", name: "Calendar for Mac", attributes: {} } },
    next: "info",
  },
  done: {
    page: "info.ftl",
    with: {
      messageHeader: "oauth2DeviceVerificationCompleteHeader",
      message: { type: "success", summary: "Go back to Kodosi." },
      skipLink: true,
      client: kodosiApp,
    },
  },
  "denied-info": {
    page: "info.ftl",
    with: {
      messageHeader: undefined,
      message: {
        type: "error",
        summary: "Nothing changed. To try again, start the sign-in in Kodosi.",
      },
      requiredActions: undefined,
      client: undefined,
      skipLink: false,
    },
  },
  denied: {
    page: "error.ftl",
    with: {
      message: {
        type: "error",
        summary: "Nothing changed. To try again, start the sign-in in Kodosi.",
      },
      skipLink: true,
    },
  },

  register: {
    page: "register.ftl",
    with: {
      passwordRequired: false,
      termsAcceptanceRequired: true,
      "x-keycloakify": { messages: { termsText: terms } },
      ...turnstile(PASSES),
    },
    attributes: profile(),
    next: "inbox",
  },
  "register-rule": {
    page: "register.ftl",
    with: {
      passwordRequired: false,
      termsAcceptanceRequired: true,
      "x-keycloakify": { messages: { termsText: terms } },
    },
    attributes: profile({ username: "maya.chen" }),
    next: "inbox",
  },
  "register-taken": {
    page: "register.ftl",
    with: {
      passwordRequired: false,
      ...refused("username", "That username is taken. Pick a different one."),
    },
    attributes: profile(maya),
    next: "inbox",
  },
  "register-blocked": {
    page: "register.ftl",
    with: {
      passwordRequired: false,
      termsAcceptanceRequired: true,
      "x-keycloakify": { messages: { termsText: terms } },
      ...turnstile(BLOCKS),
    },
    attributes: profile(maya),
    next: "inbox",
  },
  "register-ask": {
    page: "register.ftl",
    with: { passwordRequired: false, ...turnstile(ASKS) },
    attributes: profile(maya),
    next: "inbox",
  },
  "register-refused": {
    page: "register.ftl",
    with: {
      passwordRequired: false,
      termsAcceptanceRequired: true,
      "x-keycloakify": { messages: { termsText: terms } },
      ...turnstile(PASSES),
      ...checkRefused,
    },
    attributes: profile(maya),
    next: "inbox",
  },
  "register-password": {
    page: "register.ftl",
    attributes: profile(maya),
    with: {
      passwordPolicies: { length: 12, maxLength: 128, notUsername: true, notEmail: true },
      termsAcceptanceRequired: true,
      "x-keycloakify": { messages: { termsText: terms } },
      ...refused("password", "The password must have 12 or more characters."),
    },
    next: "inbox",
  },
  welcome: {
    page: "idp-review-user-profile.ftl",
    attributes: profile({ ...maya, username: "mayachen" }),
    next: "check",
  },
  profile: { page: "login-update-profile.ftl", attributes: profile(maya), next: "info" },
  "new-email": {
    page: "update-email.ftl",
    attributes: { email: attribute("email", { value: maya.email, validators: { email: {} } }) },
    with: { isAppInitiatedAction: true },
    next: "inbox",
  },

  forgot: { page: "login-reset-password.ftl", with: injected(PASSES), next: "forgot-sent" },
  "forgot-refused": {
    page: "login-reset-password.ftl",
    with: { ...injected(PASSES), ...checkRefused },
    next: "forgot-sent",
  },
  "check-page": {
    page: "turnstile-form.ftl",
    with: { ...turnstile(PASSES), isResetFlow: true },
    next: "forgot-sent",
  },
  "forgot-sent": {
    page: "login.ftl",
    with: {
      ...open,
      message: { type: "success", summary: "If the account exists, a link is on its way to you." },
    },
  },
  "new-password": {
    page: "login-update-password.ftl",
    with: {
      passwordPolicies: { length: 12, maxLength: 128, notUsername: true, notEmail: true },
      auth: person,
    },
    next: "info",
  },
  inbox: { page: "login-verify-email.ftl", with: { user: { email: maya.email } } },
  "inbox-wait": {
    page: "login-verify-email.ftl",
    with: {
      user: { email: maya.email },
      message: { type: "error", summary: "Wait 28 seconds, then send it again." },
    },
  },

  code: { page: "login-otp.ftl", with: { auth: person, ...oneApp }, next: "check" },
  "code-apps": {
    page: "login-otp.ftl",
    with: {
      auth: person,
      otpLogin: {
        userOtpCredentials: [
          { id: "a", userLabel: "iPhone" },
          { id: "b", userLabel: "Work phone" },
        ],
        selectedCredentialId: "a",
      },
    },
  },
  "code-wrong": {
    page: "login-otp.ftl",
    with: {
      auth: person,
      ...oneApp,
      ...refused("totp", "That code is not right. Type the code that the app shows now."),
    },
  },
  authenticator: { page: "login-config-totp.ftl", next: "recovery-list" },
  "authenticator-key": { page: "login-config-totp.ftl", with: { mode: "manual" } },
  "recovery-list": { page: "login-recovery-authn-code-config.ftl", next: "info" },
  "recovery-code": { page: "login-recovery-authn-code-input.ftl", with: { auth: person } },
  passkey: { page: "webauthn-authenticate.ftl", with: { auth: person } },
  "passkey-new": { page: "webauthn-register.ftl", with: { isAppInitiatedAction: true } },
  "passkey-failed": {
    page: "webauthn-error.ftl",
    with: { message: { type: "error", summary: "Your computer did not answer in time." } },
  },
  ways: {
    page: "select-authenticator.ftl",
    with: {
      auth: {
        ...person,
        authenticationSelections: [
          { authExecId: "1", displayName: "webauthn-passwordless-display-name", helpText: "" },
          {
            authExecId: "2",
            displayName: "auth-username-password-form-display-name",
            helpText: "",
          },
          { authExecId: "3", displayName: "otp-display-name", helpText: "" },
          {
            authExecId: "4",
            displayName: "auth-recovery-authn-code-form-display-name",
            helpText: "",
          },
        ],
      },
    },
  },

  info: {
    page: "info.ftl",
    with: {
      messageHeader: undefined,
      message: { type: "success", summary: "Your password is new." },
      requiredActions: undefined,
      actionUri: "#",
      client: kodosiApp,
    },
  },
  error: {
    page: "error.ftl",
    with: {
      message: { type: "error", summary: "This link is not good any more. Ask for a new one." },
    },
  },
  expired: { page: "login-page-expired.ftl" },
  "sign-out": { page: "logout-confirm.ftl", next: "signed-out" },
  "signed-out": {
    page: "info.ftl",
    with: {
      messageHeader: undefined,
      message: { type: "success", summary: "You are signed out" },
      requiredActions: undefined,
      skipLink: true,
    },
  },
  terms: { page: "terms.ftl" },
  remove: { page: "delete-credential.ftl", with: { credentialLabel: "iPhone" } },
  delete: {
    page: "delete-account-confirm.ftl",
    with: { triggered_from_aia: true },
    next: "deleted",
  },
  deleted: {
    page: "info.ftl",
    with: {
      messageHeader: undefined,
      message: { type: "success", summary: "Your account is deleted." },
      requiredActions: undefined,
      skipLink: true,
    },
  },
  link: {
    page: "login-idp-link-confirm.ftl",
    with: { idpAlias: "github", idpDisplayName: "GitHub" },
  },
  "link-email": {
    page: "login-idp-link-email.ftl",
    with: { idpAlias: "github", brokerContext: { username: maya.email } },
  },
  other: { page: "login-x509-info.ftl" },
  account: { page: "info.ftl", account: "full" },
  "account-new": { page: "info.ftl", account: "new" },
  "account-down": { page: "info.ftl", account: "down" },
};

let sample: Promise<AccountService> | undefined;

function openSample(): Promise<AccountService> {
  const query = new URLSearchParams(location.search);
  const scene = SCENES[query.get("scene") ?? ""];
  const bare = query.has("bare") ? "&bare=1" : "";
  sample ??= Promise.resolve(
    sampleAccount(scene?.account ?? "", (target) => `?scene=${target}${bare}`),
  );
  return sample;
}

function Sheet({ names }: { names: string[] }) {
  sessionStorage.removeItem("kodosi.carry");
  document.getElementById("carry")?.remove();
  document.getElementById("logo")?.remove();
  return (
    <div className="flex flex-wrap gap-4 p-4">
      {names.map((name) => (
        <figure key={name} className="m-0">
          <figcaption className="pb-1 text-caption text-ink-muted">{name}</figcaption>
          {/* oxlint-disable-next-line react/iframe-missing-sandbox -- each frame shows a page of this dev server itself */}
          <iframe
            title={name}
            src={`?scene=${name}&bare=1${SCENES[name]?.tail ?? ""}`}
            className="h-[640px] w-[390px] rounded-xl border"
          />
        </figure>
      ))}
    </div>
  );
}

export function Gallery() {
  const query = new URLSearchParams(location.search);
  const name = query.get("scene") ?? "sign-in";
  const bare = query.has("bare");
  const sheet = query.get("sheet");
  const scene = SCENES[name] ?? (SCENES["sign-in"] as Scene);
  const to = (target: string) =>
    `?scene=${target}${bare ? "&bare=1" : ""}${SCENES[target]?.tail ?? ""}`;
  if (scene.code) keepCode(CODE);
  else dropCode();
  const kcContext = getKcContextMock({
    pageId: scene.page,
    overrides: {
      realm: { displayName: "Kodosi" },
      locale: query.has("languages") ? undefined : { supported: [], currentLanguageTag: "en" },
      ...scene.with,
    } as never,
  });
  if (scene.attributes)
    (kcContext as { profile?: unknown }).profile = { attributesByName: scene.attributes };

  useEffect(() => {
    const send = (event: SubmitEvent) => {
      if (event.defaultPrevented) return;
      event.preventDefault();
      window.setTimeout(() => location.assign(to(scene.next ?? name)), 700);
    };
    window.addEventListener("submit", send);
    return () => window.removeEventListener("submit", send);
  });

  if (sheet !== null) {
    const all = Object.keys(SCENES);
    const [from = 0, count = all.length] = sheet.split(",").map(Number);
    return <Sheet names={all.slice(from, from + count)} />;
  }

  return (
    <>
      {scene.account ? (
        <Account kcContext={sampleContext()} open={openSample} />
      ) : (
        <KcPage kcContext={kcContext} />
      )}
      {!bare && (
        <select
          aria-label="Scene"
          value={name}
          onChange={(event) => location.assign(to(event.target.value))}
          className="fixed bottom-3 left-3 z-10 rounded-md border bg-raised px-2 py-1 text-caption text-ink-muted opacity-60 hover:opacity-100"
        >
          {Object.keys(SCENES).map((key) => (
            <option key={key}>{key}</option>
          ))}
        </select>
      )}
    </>
  );
}
