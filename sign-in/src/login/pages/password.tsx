import type { PasswordPolicies } from "keycloakify/login/KcContext";
import { Mail } from "lucide-react";
import { useRef, useState } from "react";

import { PersonCheckRow, usePersonCheck } from "../../parts/captcha";
import { ActionRow, Go, PasswordRow, SwitchRow, TextRow } from "../../parts/rows";
import { QuietLink, quietLink } from "../../parts/text";
import { IconTile } from "../../parts/tile";
import { Stage } from "../stage";
import { PolicyTags } from "./policy";
import type { Page } from "./props";

/** The person forgot the password: one field for the name that the link goes to. */
export function ResetPassword({ kcContext, i18n }: Page<"login-reset-password.ftl">) {
  const { url, realm, auth, messagesPerField } = kcContext;
  const { msgStr } = i18n;
  const wrong = messagesPerField.existsError("username");
  const label = !realm.loginWithEmailAllowed
    ? msgStr("kdsUsername")
    : msgStr(realm.registrationEmailAsUsername ? "kdsEmail" : "kdsUsernameOrEmail");
  const form = "kc-reset-password-form";
  const { check, held, hold } = usePersonCheck(kcContext, form);
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsForgotTitle")}
      lead={msgStr("kdsForgotLead")}
      wrong={wrong}
      problem={wrong ? messagesPerField.get("username") : undefined}
      quiet={wrong}
      waits={held}
      below={<QuietLink href={url.loginUrl}>{msgStr("kdsBackToSignIn")}</QuietLink>}
    >
      <form id={form} action={url.loginAction} method="post" noValidate onSubmit={hold}>
        <TextRow
          id="username"
          name="username"
          label={label}
          defaultValue={auth?.attemptedUsername ?? ""}
          autoFocus
          autoComplete="username"
          autoCapitalize="none"
          spellCheck={false}
          required
          aria-invalid={wrong}
          end={<Go label={msgStr("kdsSendLink")} />}
        />
        <PersonCheckRow check={check} label={msgStr("kdsNotRobot")} />
      </form>
    </Stage>
  );
}

/**
 * A new password. The person types it one time and can look at it: the row has the control that
 * shows it. Keycloak asks for the password two times, so the form sends the same value in its
 * second field.
 */
export function UpdatePassword({ kcContext, i18n }: Page<"login-update-password.ftl">) {
  const { url, messagesPerField, isAppInitiatedAction } = kcContext;
  const { msgStr } = i18n;
  const [password, setPassword] = useState("");
  const again = useRef<HTMLInputElement>(null);
  const wrong = messagesPerField.existsError("password", "password-confirm");
  const form = "kc-passwd-update-form";
  // Keycloakify adds the realm's rules to a page with a new password.
  const { passwordPolicies } = kcContext as { passwordPolicies?: PasswordPolicies };
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsNewPasswordTitle")}
      wrong={wrong}
      problem={wrong ? messagesPerField.getFirstError("password", "password-confirm") : undefined}
      quiet={wrong || kcContext.message?.type === "warning"}
      below={
        isAppInitiatedAction && (
          <button type="submit" form={form} name="cancel-aia" value="true" className={quietLink}>
            {msgStr("kdsCancel")}
          </button>
        )
      }
    >
      <form
        id={form}
        action={url.loginAction}
        method="post"
        noValidate
        onSubmit={(event) => {
          // A password manager can fill the row with no word to the page.
          const typed = event.currentTarget.elements.namedItem("password-new");
          if (again.current && typed instanceof HTMLInputElement) again.current.value = typed.value;
        }}
      >
        <PasswordRow
          id="password-new"
          name="password-new"
          label={msgStr("kdsNewPassword")}
          autoComplete="new-password"
          autoFocus
          required
          aria-invalid={wrong}
          onChange={(event) => setPassword(event.target.value)}
          show={msgStr("kdsShowPassword")}
          hide={msgStr("kdsHidePassword")}
          capsLock={msgStr("kdsCapsLock")}
          end={<Go label={msgStr("kdsSavePassword")} />}
        />
        <input ref={again} type="hidden" id="password-confirm" name="password-confirm" />
        <PolicyTags i18n={i18n} policies={passwordPolicies} password={password} />
        <SwitchRow
          id="logout-sessions"
          name="logout-sessions"
          value="on"
          label={msgStr("kdsSignOutOthers")}
          defaultChecked
        />
      </form>
    </Stage>
  );
}

/** A link went to the person's inbox. The page waits for them, and it can send the link again. */
export function VerifyEmail({ kcContext, i18n }: Page<"login-verify-email.ftl">) {
  const { url, user, isAppInitiatedAction } = kcContext;
  const { msgStr } = i18n;
  // The address that Keycloak sent the link to. An action that the account page asked for has
  // sent no link yet when Keycloak names no address.
  const sentTo = (kcContext as { verifyEmail?: string }).verifyEmail;
  const address = sentTo ?? user?.email;
  const asks = !!isAppInitiatedAction && !sentTo;
  const form = "kc-verify-email-form";
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr(asks ? "kdsConfirmEmailTitle" : "kdsInboxTitle")}
      lead={
        asks && address
          ? msgStr("kdsConfirmEmailLead", address)
          : address
            ? msgStr("kdsInboxLead", address)
            : msgStr("kdsInboxLeadPlain")
      }
      below={
        isAppInitiatedAction && (
          <button
            type="submit"
            form={form}
            name="cancel-aia"
            value="true"
            formNoValidate
            className={quietLink}
          >
            {msgStr("kdsCancel")}
          </button>
        )
      }
    >
      {isAppInitiatedAction ? (
        <form id={form} action={url.loginAction} method="post">
          <ActionRow
            tile={<IconTile tint="teal" icon={<Mail />} />}
            title={msgStr(sentTo ? "kdsSendAgain" : "kdsSend")}
          />
        </form>
      ) : (
        <ActionRow
          href={url.loginAction}
          tile={<IconTile tint="teal" icon={<Mail />} />}
          title={msgStr("kdsSendAgain")}
        />
      )}
    </Stage>
  );
}
