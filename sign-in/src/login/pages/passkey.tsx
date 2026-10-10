import { useScript as usePasskeyAtName } from "keycloakify/login/pages/LoginPasskeysConditionalAuthenticate.useScript";
import { useScript as usePasskey } from "keycloakify/login/pages/WebauthnAuthenticate.useScript";
import { useScript as useNewPasskey } from "keycloakify/login/pages/WebauthnRegister.useScript";
import { Fingerprint, RotateCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { ActionRow, Go, SignOutOthers, TextRow } from "../../parts/rows";
import { quietLink, Way } from "../../parts/text";
import { IconTile } from "../../parts/tile";
import { Stage } from "../stage";
import type { Page } from "./props";

/** Keycloak's passkey scripts find their button by this name. */
const BUTTON = "authenticateWebAuthnButton";

function Answer({ action, known }: { action: string; known?: string[] | undefined }) {
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

/** The tile of a passkey. While the computer asks the person, the copper dot of the app breathes
    on it: the page waits for them. */
function PasskeyTile({ asks }: { asks: boolean }) {
  return (
    <span className="relative">
      <IconTile tint="copper" icon={<Fingerprint />} />
      {asks && (
        <span className="breathe absolute -top-0.5 -right-0.5 size-2 rounded-full bg-accent text-accent ring-2 ring-raised" />
      )}
    </span>
  );
}

/** The person signs in with the passkey of their computer. */
export function Passkey({ kcContext, i18n }: Page<"webauthn-authenticate.ftl">) {
  const { url, authenticators } = kcContext;
  const { msgStr } = i18n;
  const [asks, setAsks] = useState(false);
  usePasskey({ authButtonId: BUTTON, kcContext, i18n });
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsPasskeyTitle")}
      lead={msgStr("kdsPasskeyLead")}
    >
      <ActionRow
        id={BUTTON}
        type="button"
        onClick={() => setAsks(true)}
        tile={<PasskeyTile asks={asks} />}
        title={msgStr("kdsPasskeyUse")}
        detail={asks ? msgStr("kdsPasskeyWaiting") : undefined}
      />
      <Answer
        action={url.loginAction}
        known={authenticators.authenticators.map((one) => one.credentialId)}
      />
    </Stage>
  );
}

/** The name first, with the passkey as the short way. */
export function PasskeyAtName({
  kcContext,
  i18n,
}: Page<"login-passkeys-conditional-authenticate.ftl">) {
  const { url, realm, login, usernameHidden, messagesPerField, authenticators } = kcContext;
  const { msgStr } = i18n;
  usePasskeyAtName({ authButtonId: BUTTON, kcContext, i18n });
  const wrong = messagesPerField.existsError("username");
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsSignIn")}
      wrong={wrong}
      problem={wrong ? messagesPerField.get("username") : undefined}
      quiet={wrong}
      below={
        <Way id={BUTTON} icon={<Fingerprint aria-hidden />}>
          {msgStr("kdsPasskeySignIn")}
        </Way>
      }
    >
      <Answer
        action={url.loginAction}
        known={authenticators?.authenticators.map((one) => one.credentialId)}
      />
      {realm.password && !usernameHidden && (
        <form id="kc-form-login" action={url.loginAction} method="post" noValidate>
          <TextRow
            id="username"
            name="username"
            label={msgStr("kdsUsernameOrEmail")}
            defaultValue={login.username ?? ""}
            autoFocus
            autoComplete="username webauthn"
            autoCapitalize="none"
            spellCheck={false}
            required
            aria-invalid={wrong}
            end={<Go label={msgStr("kdsContinue")} name="login" id="kc-login" />}
          />
        </form>
      )}
    </Stage>
  );
}

/** The computer that the person uses now: the first name of a new passkey. */
function computerName(): string {
  const agent = navigator.userAgent;
  for (const name of ["iPhone", "iPad", "Android", "Mac", "Windows", "Linux"])
    if (agent.includes(name)) return name;
  return "";
}

/** A new passkey for this computer. */
export function NewPasskey({ kcContext, i18n }: Page<"webauthn-register.ftl">) {
  const { url, isSetRetry, isAppInitiatedAction } = kcContext;
  const { msgStr } = i18n;
  const [asks, setAsks] = useState(false);
  const name = useRef<HTMLInputElement>(null);
  useNewPasskey({ authButtonId: BUTTON, kcContext, i18n });
  useEffect(() => {
    // Keycloak's script asks for the name of the passkey in a box of the browser
    // (webauthnRegister.js calls window.prompt). The page has a row for the name, and the row
    // answers in place of the box.
    const box = window.prompt;
    window.prompt = (_question, first) => name.current?.value.trim() || first || "";
    return () => {
      window.prompt = box;
    };
  }, []);
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsPasskeyNewTitle")}
      lead={msgStr("kdsPasskeyNewLead")}
      quiet={kcContext.message?.type === "warning"}
      below={
        !isSetRetry &&
        isAppInitiatedAction && (
          <form action={url.loginAction} id="kc-webauthn-settings-form" method="post">
            <button
              type="submit"
              id="cancelWebAuthnAIA"
              name="cancel-aia"
              value="true"
              className={quietLink}
            >
              {msgStr("kdsNotNow")}
            </button>
          </form>
        )
      }
    >
      <form id="register" action={url.loginAction} method="post">
        <TextRow
          ref={name}
          id="passkey-name"
          label={msgStr("kdsPasskeyName")}
          defaultValue={computerName()}
          autoComplete="off"
          maxLength={64}
          // Enter in the one field of a form sends the form. Here it starts the passkey.
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            document.getElementById(BUTTON)?.click();
          }}
        />
        <ActionRow
          id={BUTTON}
          type="button"
          onClick={() => setAsks(true)}
          tile={<PasskeyTile asks={asks} />}
          title={msgStr("kdsPasskeyCreate")}
          detail={asks ? msgStr("kdsPasskeyWaiting") : undefined}
        />
        <SignOutOthers shown={!!isAppInitiatedAction} label={msgStr("kdsSignOutOthers")} />
        <input type="hidden" id="clientDataJSON" name="clientDataJSON" />
        <input type="hidden" id="attestationObject" name="attestationObject" />
        <input type="hidden" id="publicKeyCredentialId" name="publicKeyCredentialId" />
        <input type="hidden" id="authenticatorLabel" name="authenticatorLabel" />
        <input type="hidden" id="transports" name="transports" />
        <input type="hidden" id="authenticatorAttachment" name="authenticatorAttachment" />
        <input type="hidden" id="error" name="error" />
      </form>
    </Stage>
  );
}

/** The computer did not give its passkey: the person tries again, or takes a different way. */
export function PasskeyFailed({ kcContext, i18n }: Page<"webauthn-error.ftl">) {
  const { url, isAppInitiatedAction, execution } = kcContext;
  const { msgStr } = i18n;
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsPasskeyFailedTitle")}
      sign="failed"
      below={
        isAppInitiatedAction && (
          <form action={url.loginAction} id="kc-webauthn-settings-form" method="post">
            <button
              type="submit"
              id="cancelWebAuthnAIA"
              name="cancel-aia"
              value="true"
              className={quietLink}
            >
              {msgStr("kdsCancel")}
            </button>
          </form>
        )
      }
    >
      <form id="kc-error-credential-form" action={url.loginAction} method="post">
        <ActionRow
          id="kc-try-again"
          tile={<IconTile tint="copper" icon={<RotateCw />} />}
          title={msgStr("kdsTryAgain")}
        />
        <input type="hidden" id="executionValue" name="authenticationExecution" value={execution} />
        <input type="hidden" id="isSetRetry" name="isSetRetry" value="retry" />
      </form>
    </Stage>
  );
}
