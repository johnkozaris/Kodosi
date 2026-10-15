import type { PasswordPolicies } from "keycloakify/login/KcContext";
import { Check, Mail, SquareArrowOutUpRight } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import { ActionRow, Go, PasswordRow, SignOutOthers, TextRow } from "../../parts/rows";
import { QuietLink, quietLink } from "../../parts/text";
import { IconTile } from "../../parts/tile";
import { PersonCheckRow, usePersonCheck } from "../captcha";
import { nameLabel, valueIn } from "../i18n";
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
          end={<Go label={msgStr("kdsSendLink")} />}
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
          // Keycloak asks for password-confirm, and a password manager can fill the row silently.
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

const INBOXES = [
  { name: "Gmail", href: "https://mail.google.com/", ends: ["gmail.com", "googlemail.com"] },
  {
    name: "Outlook",
    href: "https://outlook.live.com/mail/",
    ends: ["outlook.com", "hotmail.com", "live.com", "msn.com"],
  },
  { name: "iCloud Mail", href: "https://www.icloud.com/mail", ends: ["icloud.com", "me.com"] },
  {
    name: "Proton Mail",
    href: "https://mail.proton.me/",
    ends: ["proton.me", "protonmail.com", "pm.me"],
  },
  { name: "Yahoo Mail", href: "https://mail.yahoo.com/", ends: ["yahoo.com", "ymail.com"] },
  { name: "Fastmail", href: "https://app.fastmail.com/", ends: ["fastmail.com", "fastmail.fm"] },
];

function inboxOf(address: string | undefined) {
  const domain = address?.split("@")[1]?.trim().toLowerCase();
  return domain ? INBOXES.find((one) => one.ends.includes(domain)) : undefined;
}

const SENT = "kodosi.sent";

function sentJustNow(): boolean {
  try {
    const at = Number(sessionStorage.getItem(SENT) ?? 0);
    sessionStorage.removeItem(SENT);
    return Date.now() - at < 15_000;
  } catch {
    return false;
  }
}

function markSent() {
  try {
    sessionStorage.setItem(SENT, `${Date.now()}`);
  } catch {}
}

export function VerifyEmail({ kcContext, i18n }: Page<"login-verify-email.ftl">) {
  const { url, user, isAppInitiatedAction } = kcContext;
  const { msgStr } = i18n;
  // Keycloak names verifyEmail only once it has sent a link.
  const sentTo = (kcContext as { verifyEmail?: string }).verifyEmail;
  const address = sentTo ?? user?.email;
  const asks = !!isAppInitiatedAction && !sentTo;
  const inbox = asks ? undefined : inboxOf(address);
  const form = "kc-verify-email-form";
  const [wait, setWait] = useState(
    () =>
      Number.parseInt(
        valueIn(i18n, kcContext.message?.summary, "emailVerifySendCooldown") ?? "",
        10,
      ) || 0,
  );
  const [again, setAgain] = useState(() => sentJustNow() && wait === 0);
  useEffect(() => {
    if (!again) return;
    const timer = window.setTimeout(() => setAgain(false), 2600);
    return () => clearTimeout(timer);
  }, [again]);
  useEffect(() => {
    if (wait <= 0) return;
    const timer = window.setTimeout(() => setWait((now) => now - 1), 1000);
    return () => clearTimeout(timer);
  }, [wait]);
  const resend =
    wait > 0 ? (
      <ActionRow
        type="button"
        disabled
        tile={<IconTile tint="teal" icon={<Mail />} />}
        title={msgStr("kdsSendAgain")}
        detail={<span className="tabular-nums">{msgStr("kdsSendAgainIn", `${wait}`)}</span>}
      />
    ) : again ? (
      <ActionRow
        type="button"
        disabled
        tile={<IconTile tint="green" icon={<Check />} />}
        title={<span className="pop inline-block">{msgStr("kdsSentAgain")}</span>}
      />
    ) : isAppInitiatedAction ? (
      <form id={form} action={url.loginAction} method="post" onSubmit={markSent}>
        <ActionRow
          tile={<IconTile tint="teal" icon={<Mail />} />}
          title={msgStr(sentTo ? "kdsSendAgain" : "kdsSendLink")}
        />
      </form>
    ) : (
      <ActionRow
        href={url.loginAction}
        onClick={markSent}
        tile={<IconTile tint="teal" icon={<Mail />} waits={!inbox} />}
        title={msgStr("kdsSendAgain")}
      />
    );
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr(asks ? "kdsConfirmEmailTitle" : "kdsInboxTitle")}
      quiet={kcContext.message?.type !== "error" || wait > 0}
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
      {inbox && (
        <ActionRow
          href={inbox.href}
          target="_blank"
          rel="noreferrer"
          tile={<IconTile tint="teal" icon={<Mail />} waits />}
          title={msgStr("kdsOpenMail", inbox.name)}
          end={<SquareArrowOutUpRight className="size-4 shrink-0 text-ink-faint" aria-hidden />}
        />
      )}
      {resend}
    </Stage>
  );
}
