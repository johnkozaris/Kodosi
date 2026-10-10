import {
  Check,
  Fingerprint,
  KeyRound,
  LifeBuoy,
  Link2,
  Mail,
  Smartphone,
  UserRound,
} from "lucide-react";
import type { ReactNode } from "react";

import { ActionRow } from "../../parts/rows";
import { plain } from "../../parts/text";
import { IconTile, type Tint } from "../../parts/tile";
import type { I18n } from "../i18n";
import { Stage } from "../stage";
import type { Page } from "./props";

/** A way of sign-in, from the name that Keycloak gives its step. */
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

/** The person has more than one way to sign in, and chooses. */
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

/**
 * Keycloak's sentence of this page names the detail that an account has already, and its value:
 * the username or the email. The words of the page take them from it.
 */
function duplicateOf(i18n: I18n, said: string | undefined): { detail: string; value: string } {
  const text = plain(said ?? "");
  for (const detail of ["username", "email"]) {
    const [before = "", after = ""] = i18n
      .msgStr("federatedIdentityConfirmLinkMessage", detail, "\u0000")
      .split("\u0000");
    if (text.startsWith(before) && text.endsWith(after))
      return { detail, value: text.slice(before.length, text.length - after.length) };
  }
  return { detail: "email", value: "" };
}

/**
 * The first sign-in with Google, GitHub or Apple found an account that has the same email, or the
 * same username. With the same email, the person adds the service to that account. With the same
 * username, the name is most likely someone else's: the person picks a different one, or adds the
 * service to the account when it is theirs. Keycloak then asks them to prove it.
 */
export function LinkAccount({ kcContext, i18n }: Page<"login-idp-link-confirm.ftl">) {
  const { url, idpAlias, message } = kcContext;
  const { msgStr } = i18n;
  // Keycloak also gives the name that the realm chose for the service. Keycloakify's type of
  // this page has only the short name.
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

/**
 * Keycloak sent a link to the email of the account that the service joins. The page does not
 * know that address, and the person does.
 */
export function LinkByEmail({ kcContext, i18n }: Page<"login-idp-link-email.ftl">) {
  const { url, idpAlias } = kcContext;
  const { msgStr } = i18n;
  const service = (kcContext as { idpDisplayName?: string }).idpDisplayName || idpAlias;
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
        tile={<IconTile tint="green" icon={<Check />} />}
        title={msgStr("kdsLinkVerified")}
      />
      <ActionRow
        href={url.loginAction}
        tile={<IconTile tint="teal" icon={<Mail />} />}
        title={msgStr("kdsSendAgain")}
      />
    </Stage>
  );
}
