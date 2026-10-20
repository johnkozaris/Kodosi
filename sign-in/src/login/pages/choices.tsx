import {
  Check,
  Fingerprint,
  KeyRound,
  LifeBuoy,
  Link2,
  Mail,
  Smartphone,
  Undo2,
  UserRound,
} from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";

import { ActionRow } from "../../parts/rows";
import { IconTile, type Tint } from "../../parts/tile";
import { type I18n, valueIn } from "../i18n";
import { Stage } from "../stage";
import type { Page } from "./props";

function wayOf(i18n: I18n, name: string): { title: string; icon: ReactNode; tint: Tint } {
  const { msgStr, advancedMsgStr } = i18n;
  if (/webauthn|passkey/.test(name))
    return { title: msgStr("kdsWayPasskey"), icon: <Fingerprint />, tint: "copper" };
  if (/recovery/.test(name))
    return { title: msgStr("kdsWayRecovery"), icon: <LifeBuoy />, tint: "teal" };
  if (/otp/.test(name))
    return { title: msgStr("kdsWayAuthenticator"), icon: <Smartphone />, tint: "violet" };
  if (/password/.test(name))
    return { title: msgStr("kdsWayPassword"), icon: <KeyRound />, tint: "shell" };
  return { title: advancedMsgStr(name), icon: <KeyRound />, tint: "graphite" };
}

export function Ways({ kcContext, i18n }: Page<"select-authenticator.ftl">) {
  const { url, auth } = kcContext;
  const { msgStr } = i18n;
  return (
    <Stage kcContext={kcContext} i18n={i18n} title={msgStr("kdsWaysTitle")}>
      <form id="kc-select-credential-form" action={url.loginAction} method="post">
        {auth.authenticationSelections.map((selection) => {
          const way = wayOf(i18n, selection.displayName);
          return (
            <ActionRow
              key={selection.authExecId}
              name="authenticationExecution"
              value={selection.authExecId}
              tile={<IconTile tint={way.tint} icon={way.icon} />}
              title={way.title}
            />
          );
        })}
      </form>
    </Stage>
  );
}

function duplicateOf(i18n: I18n, said: string | undefined): { detail: string; value: string } {
  for (const detail of ["username", "email"]) {
    const value = valueIn(i18n, said, "federatedIdentityConfirmLinkMessage", detail);
    if (value !== undefined) return { detail, value };
  }
  return { detail: "email", value: "" };
}

export function LinkAccount({ kcContext, i18n }: Page<"login-idp-link-confirm.ftl">) {
  const { url, idpAlias, message } = kcContext;
  const { msgStr } = i18n;
  const service = (kcContext as { idpDisplayName?: string }).idpDisplayName || idpAlias;
  const { detail, value } = duplicateOf(i18n, message?.summary);
  const taken = detail === "username" && !!value;
  const link = (
    <ActionRow
      key="link"
      name="submitAction"
      id="linkAccount"
      value="linkAccount"
      tile={<IconTile tint={taken ? "shell" : "copper"} icon={<Link2 />} />}
      title={taken ? msgStr("kdsLinkAddTo", service, value) : msgStr("kdsLinkAdd")}
    />
  );
  const review = (
    <ActionRow
      key="review"
      name="submitAction"
      id="updateProfile"
      value="updateProfile"
      tile={<IconTile tint={taken ? "copper" : "shell"} icon={<UserRound />} />}
      title={msgStr(taken ? "kdsLinkOtherName" : "kdsLinkReview")}
    />
  );
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      {...(taken ? { tab: msgStr("kdsHandleTakenTab") } : {})}
      title={
        taken ? (
          <>
            <span className="text-accent-strong">@{value}</span> {msgStr("kdsHandleTaken")}
          </>
        ) : (
          msgStr("kdsLinkTitle", service)
        )
      }
      lead={
        taken
          ? msgStr("kdsHandleTakenLead", service)
          : value
            ? msgStr("kdsLinkLeadOf", value)
            : msgStr("kdsLinkLead")
      }
      quiet
    >
      <form id="kc-register-form" action={url.loginAction} method="post">
        {taken ? [review, link] : [link, review]}
      </form>
    </Stage>
  );
}

export function ReplaceLink({ kcContext, i18n }: Page<"login-idp-link-confirm-override.ftl">) {
  const { url, idpDisplayName, brokerContext, message } = kcContext;
  const { msgStr } = i18n;
  const linked = valueIn(i18n, message?.summary, "federatedIdentityConfirmOverrideMessage");
  const next = brokerContext?.username;
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsReplaceTitle", idpDisplayName)}
      lead={
        linked ? msgStr("kdsReplaceLead", linked) : msgStr("kdsReplaceLeadPlain", idpDisplayName)
      }
      quiet
    >
      <form id="kc-register-form" action={url.loginAction} method="post">
        <ActionRow
          name="submitAction"
          id="confirmOverride"
          value="confirmOverride"
          tile={<IconTile tint="copper" icon={<Link2 />} />}
          title={next ? msgStr("kdsReplaceUse", next) : msgStr("kdsLinkAdd")}
        />
        <ActionRow
          href={url.loginRestartFlowUrl}
          id="loginRestartLink"
          tile={<IconTile tint="shell" icon={<Undo2 />} />}
          title={linked ? msgStr("kdsReplaceKeep", linked) : msgStr("kdsReplaceKeepPlain")}
        />
      </form>
    </Stage>
  );
}

const LINK_SENT = "kodosi.link-sent";
const RESEND_AFTER = 30;

function secondsSinceSent(): number {
  try {
    return (Date.now() - Number(sessionStorage.getItem(LINK_SENT) ?? 0)) / 1000;
  } catch {
    return Number.POSITIVE_INFINITY;
  }
}

function markSent() {
  try {
    sessionStorage.setItem(LINK_SENT, `${Date.now()}`);
  } catch {}
}

export function LinkByEmail({ kcContext, i18n }: Page<"login-idp-link-email.ftl">) {
  const { url, idpAlias } = kcContext;
  const { msgStr } = i18n;
  const service = (kcContext as { idpDisplayName?: string }).idpDisplayName || idpAlias;
  // Keycloak sends one more mail each time this page address opens before the link is used.
  const [since] = useState(secondsSinceSent);
  const [again, setAgain] = useState(since < 15);
  const [wait, setWait] = useState(Math.max(0, Math.ceil(RESEND_AFTER - since)));
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
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsInboxTitle")}
      lead={msgStr("kdsLinkEmailLead", service)}
      quiet
    >
      <ActionRow
        href={url.loginAction}
        onClick={markSent}
        tile={<IconTile tint="green" icon={<Check />} />}
        title={msgStr("kdsLinkVerified")}
      />
      {again ? (
        <ActionRow
          type="button"
          disabled
          tile={<IconTile tint="green" icon={<Check />} />}
          title={<span className="pop inline-block">{msgStr("kdsSentAgain")}</span>}
        />
      ) : wait > 0 ? (
        <ActionRow
          type="button"
          disabled
          tile={<IconTile tint="teal" icon={<Mail />} />}
          title={msgStr("kdsSendAgain")}
          detail={<span className="tabular-nums">{msgStr("kdsSendAgainIn", `${wait}`)}</span>}
        />
      ) : (
        <ActionRow
          href={url.loginAction}
          onClick={markSent}
          tile={<IconTile tint="teal" icon={<Mail />} />}
          title={msgStr("kdsSendAgain")}
        />
      )}
    </Stage>
  );
}
