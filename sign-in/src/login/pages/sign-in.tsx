import { useScript as usePasskeyFill } from "keycloakify/login/pages/Login.useScript";
import { useScript as usePasskeyFillAtPassword } from "keycloakify/login/pages/LoginPassword.useScript";
import { useScript as usePasskeyFillAtName } from "keycloakify/login/pages/LoginUsername.useScript";
import { Fingerprint } from "lucide-react";
import { type ReactNode, useState } from "react";

import { BrandMark } from "../../parts/brands";
import { CodeTiles } from "../../parts/code";
import { Go, PasswordRow, SwitchRow, TextRow } from "../../parts/rows";
import { Note, QuietLink, Way } from "../../parts/text";
import { heldCode } from "../device";
import { type I18n, isMessage } from "../i18n";
import { Stage } from "../stage";
import { keepWay, lastWay } from "../ways";
import type { Page } from "./props";

/** Keycloak's passkey script finds this button by its name. */
const PASSKEY_BUTTON = "authenticateWebAuthnButton";
/** Keycloak's sentences for a passkey that did not answer or did not fit: a person who closed the
    passkey box of the browser gets the first. */
const PASSKEY_PROBLEMS = [
  "webauthn-error-api-get",
  "webauthn-error-auth-verification",
  "webauthn-error-different-user",
  "webauthn-error-user-not-found",
];

/** What Keycloak said about the passkey, when it said something about it. */
function passkeyProblem(
  kcContext: { message?: { summary: string; type: string } | undefined },
  i18n: I18n,
): string | undefined {
  const said = kcContext.message;
  return said?.type === "error" &&
    PASSKEY_PROBLEMS.some((key) => isMessage(i18n, said.summary, key))
    ? said.summary
    : undefined;
}

function nameLabel(
  i18n: I18n,
  realm: { loginWithEmailAllowed: boolean; registrationEmailAsUsername: boolean },
) {
  if (!realm.loginWithEmailAllowed) return i18n.msgStr("kdsUsername");
  return i18n.msgStr(realm.registrationEmailAsUsername ? "kdsEmail" : "kdsUsernameOrEmail");
}

type Provider = { alias: string; loginUrl: string; displayName: string; providerId?: string };

/**
 * Under the capsule: the forgotten password, a passkey, the sign-in services of other companies,
 * and the way to a new account. The service that this browser used the last time comes first.
 */
function OtherWays({
  i18n,
  forgot,
  passkey,
  problem,
  providers,
  register,
  onPasskey,
}: {
  i18n: I18n;
  forgot?: string | undefined;
  passkey: boolean;
  /** What went wrong with the passkey: it shows under the passkey's button. */
  problem?: string | undefined;
  providers?: Provider[] | undefined;
  register?: string | undefined;
  onPasskey: () => void;
}) {
  const { msgStr } = i18n;
  const [last] = useState(lastWay);
  const sorted = (providers ?? []).toSorted(
    (a, b) => Number(b.alias === last) - Number(a.alias === last),
  );
  return (
    <>
      {forgot && <QuietLink href={forgot}>{msgStr("kdsForgot")}</QuietLink>}
      {(passkey || sorted.length > 0) && (
        <div className="mt-2 flex w-full flex-col gap-2">
          {passkey && (
            <Way
              id={PASSKEY_BUTTON}
              icon={<Fingerprint aria-hidden />}
              onClick={onPasskey}
              aria-describedby={problem ? "passkey-note" : undefined}
            >
              {msgStr("kdsPasskeySignIn")}
            </Way>
          )}
          <div aria-live="polite">
            {passkey && problem && <Note tone="danger" text={problem} id="passkey-note" />}
          </div>
          {sorted.length > 0 && (
            <div className="grid grid-cols-[repeat(auto-fit,minmax(104px,1fr))] gap-2">
              {sorted.map((provider) => (
                <Way
                  key={provider.alias}
                  id={`social-${provider.alias}`}
                  href={provider.loginUrl}
                  aria-label={msgStr("kdsContinueWith", provider.displayName)}
                  icon={<BrandMark {...provider} />}
                  mark={provider.alias === last ? msgStr("kdsLastUsed") : undefined}
                  onClick={() => keepWay(provider.alias)}
                >
                  {provider.displayName}
                </Way>
              ))}
            </div>
          )}
        </div>
      )}
      {register && (
        <p className="mt-3 text-footnote text-ink-muted">
          {msgStr("kdsNewHere")} <QuietLink href={register}>{msgStr("kdsMakeAccount")}</QuietLink>
        </p>
      )}
    </>
  );
}

/** The forms that Keycloak's passkey script fills and sends. */
function PasskeyForms({ action, known }: { action: string; known?: string[] | undefined }) {
  return (
    <>
      <form id="webauth" action={action} method="post" hidden>
        <input type="hidden" id="clientDataJSON" name="clientDataJSON" />
        <input type="hidden" id="authenticatorData" name="authenticatorData" />
        <input type="hidden" id="signature" name="signature" />
        <input type="hidden" id="credentialId" name="credentialId" />
        <input type="hidden" id="userHandle" name="userHandle" />
        <input type="hidden" id="error" name="error" />
      </form>
      {!!known?.length && (
        <form id="authn_select" hidden>
          {known.map((id) => (
            <input key={id} type="hidden" name="authn_use_chk" readOnly value={id} />
          ))}
        </form>
      )}
    </>
  );
}

function Frame({
  kcContext,
  i18n,
  wrong,
  problem,
  below,
  children,
}: Page<"login.ftl" | "login-username.ftl" | "login-password.ftl"> & {
  wrong: boolean;
  problem: string | undefined;
  below: ReactNode;
  children: ReactNode;
}) {
  const { msgStr } = i18n;
  // The code shows on the sign-in of the app that has it: the same browser tab can sign in to
  // the account page a moment later.
  const app = kcContext.client.attributes["oauth2.device.authorization.grant.enabled"] === "true";
  const code = app ? heldCode() : null;
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsSignIn")}
      // The person came from the Kodosi app: the page shows the code that the app shows.
      lead={code && <CodeTiles small code={code} label={msgStr("kdsCodeFromApp")} />}
      wrong={wrong}
      problem={problem}
      // A problem of the passkey is under the passkey's button, not under the capsule.
      quiet={wrong || !!passkeyProblem(kcContext, i18n)}
      below={below}
    >
      {children}
    </Stage>
  );
}

/** The name and the password in one capsule. */
export function Login({ kcContext, i18n }: Page<"login.ftl">) {
  const { realm, url, login, usernameHidden, auth, messagesPerField, social } = kcContext;
  const { msgStr } = i18n;
  const passkey = kcContext.enableWebAuthnConditionalUI === true;
  usePasskeyFill({ webAuthnButtonId: PASSKEY_BUTTON, kcContext, i18n });
  const wrong = messagesPerField.existsError("username", "password");

  return (
    <Frame
      kcContext={kcContext}
      i18n={i18n}
      wrong={wrong}
      problem={wrong ? messagesPerField.getFirstError("username", "password") : undefined}
      below={
        <OtherWays
          i18n={i18n}
          forgot={realm.resetPasswordAllowed ? url.loginResetCredentialsUrl : undefined}
          passkey={passkey}
          problem={passkeyProblem(kcContext, i18n)}
          providers={social?.providers}
          register={
            realm.password && realm.registrationAllowed && !kcContext.registrationDisabled
              ? url.registrationUrl
              : undefined
          }
          onPasskey={() => keepWay(null)}
        />
      }
    >
      {realm.password && (
        <form
          id="kc-form-login"
          action={url.loginAction}
          method="post"
          noValidate
          onSubmit={() => keepWay(null)}
        >
          {!usernameHidden && (
            <TextRow
              id="username"
              name="username"
              label={nameLabel(i18n, realm)}
              defaultValue={login.username ?? ""}
              autoFocus={!login.username}
              autoComplete={passkey ? "username webauthn" : "username"}
              autoCapitalize="none"
              spellCheck={false}
              required
              aria-invalid={wrong}
            />
          )}
          <PasswordRow
            id="password"
            name="password"
            label={msgStr("kdsPassword")}
            autoComplete="current-password"
            autoFocus={!!login.username || !!usernameHidden}
            required
            aria-invalid={wrong}
            show={msgStr("kdsShowPassword")}
            hide={msgStr("kdsHidePassword")}
            capsLock={msgStr("kdsCapsLock")}
            end={<Go label={msgStr("kdsSignIn")} name="login" id="kc-login" />}
          />
          {realm.rememberMe && !usernameHidden && (
            <SwitchRow
              id="rememberMe"
              name="rememberMe"
              label={msgStr("kdsKeepSignedIn")}
              defaultChecked={!!login.rememberMe}
            />
          )}
          <input
            type="hidden"
            id="id-hidden-input"
            name="credentialId"
            value={auth.selectedCredential}
          />
        </form>
      )}
      {passkey && (
        <PasskeyForms
          action={url.loginAction}
          known={kcContext.authenticators?.authenticators.map((one) => one.credentialId)}
        />
      )}
    </Frame>
  );
}

/** The name first: Keycloak then knows which ways of sign-in the person has. */
export function LoginUsername({ kcContext, i18n }: Page<"login-username.ftl">) {
  const { realm, url, login, usernameHidden, messagesPerField, social } = kcContext;
  const { msgStr } = i18n;
  const passkey = kcContext.enableWebAuthnConditionalUI === true;
  usePasskeyFillAtName({ webAuthnButtonId: PASSKEY_BUTTON, kcContext, i18n });
  const wrong = messagesPerField.existsError("username");

  return (
    <Frame
      kcContext={kcContext}
      i18n={i18n}
      wrong={wrong}
      problem={wrong ? messagesPerField.getFirstError("username") : undefined}
      below={
        <OtherWays
          i18n={i18n}
          passkey={passkey}
          problem={passkeyProblem(kcContext, i18n)}
          providers={social?.providers}
          register={
            realm.password && realm.registrationAllowed && !kcContext.registrationDisabled
              ? url.registrationUrl
              : undefined
          }
          onPasskey={() => keepWay(null)}
        />
      }
    >
      {realm.password && (
        <form
          id="kc-form-login"
          action={url.loginAction}
          method="post"
          noValidate
          onSubmit={() => keepWay(null)}
        >
          {!usernameHidden && (
            <TextRow
              id="username"
              name="username"
              label={nameLabel(i18n, realm)}
              defaultValue={login.username ?? ""}
              autoFocus
              autoComplete={passkey ? "username webauthn" : "username"}
              autoCapitalize="none"
              spellCheck={false}
              required
              aria-invalid={wrong}
              end={<Go label={msgStr("kdsContinue")} name="login" id="kc-login" />}
            />
          )}
          {realm.rememberMe && !usernameHidden && (
            <SwitchRow
              id="rememberMe"
              name="rememberMe"
              label={msgStr("kdsKeepSignedIn")}
              defaultChecked={!!login.rememberMe}
            />
          )}
        </form>
      )}
      {passkey && (
        <PasskeyForms
          action={url.loginAction}
          known={kcContext.authenticators?.authenticators.map((one) => one.credentialId)}
        />
      )}
    </Frame>
  );
}

/** The password of the person whom the page names. */
export function LoginPassword({ kcContext, i18n }: Page<"login-password.ftl">) {
  const { realm, url, messagesPerField } = kcContext;
  const { msgStr } = i18n;
  const passkey = kcContext.enableWebAuthnConditionalUI === true;
  usePasskeyFillAtPassword({ webAuthnButtonId: PASSKEY_BUTTON, kcContext, i18n });
  const wrong = messagesPerField.existsError("password");

  return (
    <Frame
      kcContext={kcContext}
      i18n={i18n}
      wrong={wrong}
      problem={wrong ? messagesPerField.get("password") : undefined}
      below={
        <OtherWays
          i18n={i18n}
          forgot={realm.resetPasswordAllowed ? url.loginResetCredentialsUrl : undefined}
          passkey={passkey}
          problem={passkeyProblem(kcContext, i18n)}
          onPasskey={() => keepWay(null)}
        />
      }
    >
      <form id="kc-form-login" action={url.loginAction} method="post" noValidate>
        <PasswordRow
          id="password"
          name="password"
          label={msgStr("kdsPassword")}
          autoComplete="current-password"
          autoFocus
          required
          aria-invalid={wrong}
          show={msgStr("kdsShowPassword")}
          hide={msgStr("kdsHidePassword")}
          capsLock={msgStr("kdsCapsLock")}
          end={<Go label={msgStr("kdsSignIn")} name="login" id="kc-login" />}
        />
      </form>
      {passkey && (
        <PasskeyForms
          action={url.loginAction}
          known={kcContext.authenticators?.authenticators.map((one) => one.credentialId)}
        />
      )}
    </Frame>
  );
}
