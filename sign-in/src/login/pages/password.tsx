import type { PasswordPolicies } from "keycloakify/login/KcContext";
import { useRef, useState } from "react";

import { Go, PasswordRow, SignOutOthers, TextRow } from "../../parts/rows";
import { QuietLink, quietLink } from "../../parts/text";
import { PersonCheckRow, usePersonCheck } from "../captcha";
import { nameLabel } from "../i18n";
import { Stage } from "../stage";
import { meetsPolicy, PolicyTags } from "./policy";
import type { Page } from "./props";

export function ResetPassword({ kcContext, i18n }: Page<"login-reset-password.ftl">) {
  const { url, realm, auth, messagesPerField } = kcContext;
  const { msgStr } = i18n;
  const wrong = messagesPerField.existsError("username");
  const form = "kc-reset-password-form";
  const check = usePersonCheck(kcContext, form, i18n);
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsForgotTitle")}
      lead={msgStr("kdsForgotLead")}
      wrong={wrong || (check.refused && !check.retried)}
      problem={wrong ? messagesPerField.get("username") : undefined}
      quiet={wrong || check.refused}
      waits={check.held}
      below={<QuietLink href={url.loginUrl}>{msgStr("kdsBackToSignIn")}</QuietLink>}
    >
      <form id={form} action={url.loginAction} method="post" noValidate onSubmit={check.hold}>
        <TextRow
          id="username"
          name="username"
          label={nameLabel(i18n, realm)}
          defaultValue={auth?.attemptedUsername ?? ""}
          autoFocus
          autoComplete="username"
          autoCapitalize="none"
          spellCheck={false}
          required
          aria-invalid={wrong}
          end={<Go label={msgStr("kdsSendCode")} />}
        />
        <PersonCheckRow check={check} i18n={i18n} />
      </form>
    </Stage>
  );
}

export function UpdatePassword({ kcContext, i18n }: Page<"login-update-password.ftl">) {
  const { url, messagesPerField, isAppInitiatedAction } = kcContext;
  const { msgStr } = i18n;
  const [password, setPassword] = useState("");
  const again = useRef<HTMLInputElement>(null);
  const wrong = messagesPerField.existsError("password", "password-confirm");
  const form = "kc-passwd-update-form";
  const { passwordPolicies, username } = kcContext as {
    passwordPolicies?: PasswordPolicies;
    username?: string;
  };
  const person = { username: username ?? kcContext.auth?.attemptedUsername };
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsPasswordTitle")}
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
          // Keycloak requires password-confirm; copy from the DOM on submit in case a password
          // manager autofilled password-new without firing onChange.
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
          fits={meetsPolicy(i18n, passwordPolicies, password, person)}
          show={msgStr("kdsShowPassword")}
          hide={msgStr("kdsHidePassword")}
          capsLock={msgStr("kdsCapsLock")}
          end={<Go label={msgStr("kdsSavePassword")} />}
        />
        <input ref={again} type="hidden" id="password-confirm" name="password-confirm" />
        <PolicyTags
          i18n={i18n}
          policies={passwordPolicies}
          password={password}
          username={person.username}
        />
        <SignOutOthers shown={!!isAppInitiatedAction} label={msgStr("kdsSignOutOthers")} />
      </form>
    </Stage>
  );
}
