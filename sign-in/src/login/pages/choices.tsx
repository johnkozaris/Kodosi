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

/** The sign-in with Google, GitHub or Apple found an account with the same email. */
export function LinkAccount({ kcContext, i18n }: Page<"login-idp-link-confirm.ftl">) {
  const { url, idpAlias } = kcContext;
  const { msgStr } = i18n;
  // Keycloak also gives the name that the realm chose for the service. Keycloakify's type of
  // this page has only the short name.
  const service = (kcContext as { idpDisplayName?: string }).idpDisplayName || idpAlias;
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsLinkTitle", service)}
      lead={msgStr("kdsLinkLead")}
    >
      <form id="kc-register-form" action={url.loginAction} method="post">
        <ActionRow
          name="submitAction"
          id="linkAccount"
          value="linkAccount"
          tile={<IconTile tint="copper" icon={<Link2 />} />}
          title={msgStr("kdsLinkAdd")}
        />
        <ActionRow
          name="submitAction"
          id="updateProfile"
          value="updateProfile"
          tile={<IconTile tint="shell" icon={<UserRound />} />}
          title={msgStr("kdsLinkReview")}
        />
      </form>
    </Stage>
  );
}

export function LinkByEmail({ kcContext, i18n }: Page<"login-idp-link-email.ftl">) {
  const { url, brokerContext } = kcContext;
  const { msgStr } = i18n;
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsInboxTitle")}
      lead={msgStr("kdsLinkEmailLead", brokerContext.username)}
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
