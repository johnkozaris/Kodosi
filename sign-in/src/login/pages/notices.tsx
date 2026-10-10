import {
  ArrowRight,
  DoorClosed,
  LogOut,
  MessageSquareText,
  RotateCcw,
  SquareTerminal,
  Trash2,
  UserRoundX,
} from "lucide-react";
import { useEffect } from "react";

import { buttonClass } from "../../parts/button";
import { cn } from "../../parts/cn";
import { ActionRow, rule } from "../../parts/rows";
import { plain, QuietLink, quietLink } from "../../parts/text";
import { IconTile } from "../../parts/tile";
import { dropCode } from "../device";
import { isMessage as says } from "../i18n";
import { Stage } from "../stage";
import type { Page } from "./props";

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

/**
 * The end of an account. Keycloak deleted it and ended the session, so no link of the page goes
 * anywhere now: the page says it calmly, and the cursor of the mark is an outline.
 */
function Deleted({ kcContext, i18n }: Page<"info.ftl">) {
  const { msgStr } = i18n;
  useEffect(dropCode, []);
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsDeletedTitle")}
      lead={msgStr("kdsDeletedLead")}
      rest
      ended
      quiet
    />
  );
}

/**
 * The person said that the code was not the same, so the app was not connected. Nothing is wrong:
 * the page is calm, and it says how to try again. Keycloak shows it as an info page or as an
 * error page, with no program to go back to.
 */
function Denied({ kcContext, i18n }: Page<"info.ftl" | "error.ftl">) {
  const { msgStr } = i18n;
  useEffect(dropCode, []);
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsDeniedTitle")}
      lead={msgStr("kdsDeniedLead")}
      rest
      quiet
    />
  );
}

/** Keycloak tells the person something, and the capsule holds the way on. */
export function Info({ kcContext, i18n }: Page<"info.ftl">) {
  const { messageHeader, message, requiredActions, skipLink, pageRedirectUri, actionUri } =
    kcContext;
  // Keycloak names no program on a page whose sign-in has ended.
  const client = kcContext.client as typeof kcContext.client | undefined;
  const { msgStr, advancedMsgStr } = i18n;
  if (says(i18n, messageHeader, "oauth2DeviceVerificationCompleteHeader"))
    return <Connected kcContext={kcContext} i18n={i18n} />;
  if (says(i18n, message.summary, "userDeletedSuccessfully"))
    return <Deleted kcContext={kcContext} i18n={i18n} />;
  if (says(i18n, message.summary, "oauth2DeviceConsentDeniedMessage"))
    return <Denied kcContext={kcContext} i18n={i18n} />;

  const failed = says(i18n, messageHeader, "oauth2DeviceVerificationFailedHeader");
  const program = client?.name ? advancedMsgStr(client.name) : "Kodosi";
  const steps = requiredActions?.map((step) => advancedMsgStr(`requiredAction.${step}`)).join(", ");
  const on = skipLink
    ? undefined
    : pageRedirectUri
      ? { href: pageRedirectUri, label: msgStr("kdsBackTo", program) }
      : actionUri
        ? { href: actionUri, label: msgStr("kdsContinue") }
        : client?.baseUrl
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
  useEffect(dropCode, []);
  if (says(i18n, message.summary, "oauth2DeviceConsentDeniedMessage"))
    return <Denied kcContext={kcContext} i18n={i18n} />;
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsErrorTitle")}
      lead={plain(message.summary)}
      sign="failed"
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
  const { url, logoutConfirm } = kcContext;
  // A sign-out that no program asked for has no program to go back to.
  const client = kcContext.client as typeof kcContext.client | undefined;
  const { msgStr, advancedMsgStr } = i18n;
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsSignOutTitle")}
      below={
        !logoutConfirm.skipLink &&
        client?.baseUrl && (
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

/**
 * The person deletes their account. Four short lines say what goes and what stays, one line says
 * that it is for good, and one red button does it. The way back is the quiet one under the
 * capsule. No button has the focus from the start: a key that the person pressed for a
 * different reason must not delete an account.
 */
export function DeleteAccount({ kcContext, i18n }: Page<"delete-account-confirm.ftl">) {
  const { url, triggered_from_aia } = kcContext;
  const { msgStr } = i18n;
  const form = "kc-delete-account";
  const lines = [
    { icon: UserRoundX, text: msgStr("kdsDeleteGoes") },
    { icon: DoorClosed, text: msgStr("kdsDeleteRooms") },
    { icon: MessageSquareText, text: msgStr("kdsDeleteWords") },
    { icon: SquareTerminal, text: msgStr("kdsDeleteTerminals") },
  ];
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsDeleteTitle")}
      lead={msgStr("kdsDeleteLead")}
      below={
        triggered_from_aia && (
          <button type="submit" form={form} name="cancel-aia" value="true" className={quietLink}>
            {msgStr("kdsKeepAccount")}
          </button>
        )
      }
    >
      <ul className="flex flex-col gap-3 px-[18px] pt-5 pb-4">
        {lines.map(({ icon: Icon, text }, i) => (
          <li
            key={text}
            className="rise flex items-start gap-3 text-[14.5px] leading-[21px] text-ink"
            style={{ animationDelay: `${80 + i * 60}ms` }}
          >
            <Icon
              className="mt-0.5 size-[17px] shrink-0 text-ink-muted"
              strokeWidth={2}
              aria-hidden
            />
            <span>{text}</span>
          </li>
        ))}
      </ul>
      <form id={form} action={url.loginAction} method="post" className={cn(rule, "p-3")}>
        <button
          type="submit"
          id="kc-delete"
          className={buttonClass({ variant: "danger", size: "lg", className: "w-full" })}
        >
          <Trash2 aria-hidden />
          {msgStr("kdsDelete")}
        </button>
      </form>
    </Stage>
  );
}
