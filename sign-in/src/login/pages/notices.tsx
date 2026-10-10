import { ArrowRight, LogOut, RotateCcw, Trash2 } from "lucide-react";
import { useEffect } from "react";

import { ActionRow } from "../../parts/rows";
import { plain, QuietLink, quietLink } from "../../parts/text";
import { IconTile } from "../../parts/tile";
import { dropCode } from "../device";
import type { I18n } from "../i18n";
import { Stage } from "../stage";
import type { Page } from "./props";

/** Keycloak gives a sentence as its key, or as the words of that key. */
function says(i18n: I18n, text: string | undefined, key: Parameters<I18n["msgStr"]>[0]): boolean {
  return !!text && (text === key || plain(text) === i18n.msgStr(key));
}

/**
 * The end of the sign-in of the Kodosi app, and the end of the journey in the browser. The mark
 * is whole, as on the banner and in the last frame of the launch film, and the words are the
 * app's own: it says "You are in" at the same moment.
 */
function Connected({ kcContext, i18n }: Page<"info.ftl">) {
  const { msgStr } = i18n;
  useEffect(dropCode, []);
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsDoneTitle")}
      lead={msgStr("kdsDoneLead")}
      sign="done"
      rest
      banner
      quiet
      foot={
        <p className="rise font-mono text-caption text-ink-muted [animation-delay:0.5s]">
          {msgStr("kdsTagline")}
        </p>
      }
    />
  );
}

/** Keycloak tells the person something, and the capsule holds the way on. */
export function Info({ kcContext, i18n }: Page<"info.ftl">) {
  const { messageHeader, message, requiredActions, skipLink, pageRedirectUri, actionUri, client } =
    kcContext;
  const { msgStr, advancedMsgStr } = i18n;
  if (says(i18n, messageHeader, "oauth2DeviceVerificationCompleteHeader"))
    return <Connected kcContext={kcContext} i18n={i18n} />;

  const failed = says(i18n, messageHeader, "oauth2DeviceVerificationFailedHeader");
  const program = client.name ? advancedMsgStr(client.name) : "Kodosi";
  const steps = requiredActions?.map((step) => advancedMsgStr(`requiredAction.${step}`)).join(", ");
  const on = skipLink
    ? undefined
    : pageRedirectUri
      ? { href: pageRedirectUri, label: msgStr("kdsBackTo", program) }
      : actionUri
        ? { href: actionUri, label: msgStr("kdsContinue") }
        : client.baseUrl
          ? { href: client.baseUrl, label: msgStr("kdsBackTo", program) }
          : undefined;
  const said = plain(messageHeader ? advancedMsgStr(messageHeader) : message.summary).trim();
  // A short sentence is the line of the page. A long one goes under a line of few words.
  const short = said.length <= 34;
  const title = short ? said.replace(/\.$/, "") : msgStr(on ? "kdsOneMoreStep" : "kdsNoteTitle");
  const more = messageHeader ? plain(message.summary) : steps;
  // Keycloak says that the person is signed out on a page of its sign-out address.
  const out = location.pathname.includes("/openid-connect/logout");
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      tab={title}
      title={title}
      lead={short ? more : [said, more].filter(Boolean).join(" ")}
      {...(failed
        ? { sign: "failed" as const }
        : message.type === "success" && !out
          ? { sign: "done" as const }
          : {})}
      rest={out || failed}
      quiet
    >
      {on && (
        <ActionRow
          href={on.href}
          tile={<IconTile tint="copper" icon={<ArrowRight />} />}
          title={on.label}
        />
      )}
    </Stage>
  );
}

export function Failure({ kcContext, i18n }: Page<"error.ftl">) {
  const { message, client, skipLink } = kcContext;
  const { msgStr, advancedMsgStr } = i18n;
  // The person said that the code was not the same: nothing is wrong.
  const denied = says(i18n, message.summary, "oauth2DeviceConsentDeniedMessage");
  useEffect(dropCode, []);
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr(denied ? "kdsDeniedTitle" : "kdsErrorTitle")}
      lead={denied ? msgStr("kdsDeniedLead") : plain(message.summary)}
      {...(denied ? {} : { sign: "failed" as const })}
      rest
      quiet
    >
      {!skipLink && client?.baseUrl && (
        <ActionRow
          href={client.baseUrl}
          id="backToApplication"
          tile={<IconTile tint="copper" icon={<ArrowRight />} />}
          title={msgStr("kdsBackTo", client.name ? advancedMsgStr(client.name) : "Kodosi")}
        />
      )}
    </Stage>
  );
}

/** The sign-in waited too long. The person starts again, or goes on where they were. */
export function Expired({ kcContext, i18n }: Page<"login-page-expired.ftl">) {
  const { url } = kcContext;
  const { msgStr } = i18n;
  return (
    <Stage kcContext={kcContext} i18n={i18n} title={msgStr("kdsExpiredTitle")}>
      <ActionRow
        href={url.loginRestartFlowUrl}
        id="loginRestartLink"
        tile={<IconTile tint="copper" icon={<RotateCcw />} />}
        title={msgStr("kdsStartAgain")}
      />
      <ActionRow
        href={url.loginAction}
        id="loginContinueLink"
        tile={<IconTile tint="shell" icon={<ArrowRight />} />}
        title={msgStr("kdsContinueWhere")}
      />
    </Stage>
  );
}

export function SignOut({ kcContext, i18n }: Page<"logout-confirm.ftl">) {
  const { url, client, logoutConfirm } = kcContext;
  const { msgStr, advancedMsgStr } = i18n;
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsSignOutTitle")}
      below={
        !logoutConfirm.skipLink &&
        client.baseUrl && (
          <QuietLink href={client.baseUrl}>
            {msgStr("kdsBackTo", client.name ? advancedMsgStr(client.name) : "Kodosi")}
          </QuietLink>
        )
      }
    >
      <form action={url.logoutConfirmAction} method="post">
        <ActionRow
          name="confirmLogout"
          id="kc-logout"
          value="true"
          tile={<IconTile tint="shell" icon={<LogOut />} />}
          title={msgStr("kdsSignOut")}
        />
        <input type="hidden" name="session_code" value={logoutConfirm.code} />
      </form>
    </Stage>
  );
}

/** The person takes away one of their ways of sign-in. */
export function RemoveWay({ kcContext, i18n }: Page<"delete-credential.ftl">) {
  const { url, credentialLabel } = kcContext;
  const { msgStr } = i18n;
  const form = "kc-delete-credential";
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      tab={msgStr("kdsRemove")}
      title={msgStr("kdsRemoveTitle", credentialLabel)}
      below={
        <button type="submit" form={form} name="cancel-aia" value="true" className={quietLink}>
          {msgStr("kdsCancel")}
        </button>
      }
    >
      <form id={form} action={url.loginAction} method="post">
        <ActionRow
          name="accept"
          id="kc-accept"
          value="true"
          tile={<IconTile tint="red" icon={<Trash2 />} />}
          title={msgStr("kdsRemove")}
        />
      </form>
    </Stage>
  );
}

/** The person deletes their account. One sentence says what goes, and one row does it. */
export function DeleteAccount({ kcContext, i18n }: Page<"delete-account-confirm.ftl">) {
  const { url, triggered_from_aia } = kcContext;
  const { msgStr } = i18n;
  const form = "kc-delete-account";
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsDeleteTitle")}
      lead={msgStr("kdsDeleteLead")}
      below={
        triggered_from_aia && (
          <button type="submit" form={form} name="cancel-aia" value="true" className={quietLink}>
            {msgStr("kdsCancel")}
          </button>
        )
      }
    >
      <form id={form} action={url.loginAction} method="post">
        <ActionRow tile={<IconTile tint="red" icon={<Trash2 />} />} title={msgStr("kdsDelete")} />
      </form>
    </Stage>
  );
}
