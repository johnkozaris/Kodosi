import { Check } from "lucide-react";
import { useEffect, useState } from "react";

import { buttonClass } from "../../parts/button";
import { CodeCells, CodeTiles } from "../../parts/code";
import { QuietLink } from "../../parts/text";
import { codeFromApp, dropCode, heldCode, keepCode } from "../device";
import { Stage } from "../stage";
import type { Page } from "./props";

/** The field of Keycloak's own page for the code. */
const FIELD = "device_user_code";

/**
 * The page that takes the code of the Kodosi app. A person can type the code. The app can also
 * put the code after the "#" of this page's address: the cells then show it letter by letter and
 * the page goes on by itself, and each later step shows the code that this page sent.
 */
export function DeviceCode({ kcContext, i18n }: Page<"login-oauth2-device-verify-user-code.ftl">) {
  const { url, message } = kcContext;
  const { msgStr } = i18n;
  const wrong = message?.type === "error";
  // A code that Keycloak refused does not go a second time by itself.
  const [fromApp] = useState(() => (wrong ? null : codeFromApp()));
  // A code that Keycloak refused is not the code of this visit.
  useEffect(dropCode, []);
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsDeviceTitle")}
      lead={msgStr("kdsDeviceLead")}
      wrong={wrong}
      problem={wrong ? message.summary : undefined}
      quiet={wrong}
    >
      <form
        id="kc-user-verify-device-user-code-form"
        action={url.oauth2DeviceVerificationAction}
        method="post"
        noValidate
        onSubmit={(event) => keepCode(`${new FormData(event.currentTarget).get(FIELD) ?? ""}`)}
      >
        <CodeCells
          id="device-user-code"
          name={FIELD}
          length={8}
          kind="letters"
          half
          label={msgStr("kdsCodeFromApp")}
          invalid={wrong}
          given={fromApp}
          autoFocus={!fromApp}
        />
      </form>
    </Stage>
  );
}

/**
 * A program asks for the person's account. For the Kodosi app, this is the moment of the code:
 * the page shows the code large, and the person connects their computer when the app shows the
 * same one. With no code of this visit, the page asks to connect the app. A program that is not
 * an app with a code gets the plain question.
 */
export function Grant({ kcContext, i18n }: Page<"login-oauth-grant.ftl">) {
  const { url, oauth, client } = kcContext;
  const { msgStr, advancedMsgStr } = i18n;
  const program = client.name ? advancedMsgStr(client.name) : client.clientId;
  // Keycloak says in the client's own settings that it signs in with a code.
  const app = client.attributes["oauth2.device.authorization.grant.enabled"] === "true";
  const code = app ? heldCode() : null;
  const { tosUri, policyUri } = client.attributes;
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={
        code ? msgStr("kdsCheckTitle") : msgStr(app ? "kdsConnectTitle" : "kdsGrantTitle", program)
      }
      lead={
        code ? msgStr("kdsCheckLead", program) : app ? msgStr("kdsConnectLead", program) : undefined
      }
      below={
        (tosUri || policyUri) && (
          <span className="flex gap-5">
            {tosUri && (
              <QuietLink href={tosUri} target="_blank" rel="noreferrer">
                {msgStr("oauthGrantTos")}
              </QuietLink>
            )}
            {policyUri && (
              <QuietLink href={policyUri} target="_blank" rel="noreferrer">
                {msgStr("oauthGrantPolicy")}
              </QuietLink>
            )}
          </span>
        )
      }
    >
      {code && (
        <div className="flex justify-center px-4 pt-7 pb-3">
          <CodeTiles code={code} label={msgStr("kdsCodeFromApp")} />
        </div>
      )}
      <div className="px-[18px] pt-4 pb-3.5">
        <p className="text-footnote text-ink-muted">{msgStr("kdsGets", program)}</p>
        <ul className="mt-1.5">
          {oauth.clientScopesRequested.map((scope) => (
            <li
              key={scope.consentScreenText + (scope.dynamicScopeParameter ?? "")}
              className="flex items-start gap-2.5 py-1 text-[14.5px] text-ink"
            >
              <Check
                className="mt-[3px] size-4 shrink-0 text-ink-faint"
                strokeWidth={2.4}
                aria-hidden
              />
              <span>
                {advancedMsgStr(scope.consentScreenText)}
                {scope.dynamicScopeParameter && (
                  <span className="text-ink-muted">: {scope.dynamicScopeParameter}</span>
                )}
              </span>
            </li>
          ))}
        </ul>
      </div>
      <form
        action={url.oauthAction}
        method="post"
        className="relative flex justify-end gap-2 p-3 before:absolute before:top-0 before:right-0 before:left-[18px] before:h-px before:bg-hairline/60"
      >
        <input type="hidden" name="code" value={oauth.code} />
        <button
          type="submit"
          name="cancel"
          id="kc-cancel"
          value="true"
          className={buttonClass({ variant: "ghost" })}
        >
          {msgStr(code ? "kdsNotSame" : "kdsCancel")}
        </button>{" "}
        {/* No button has the focus from the start: a key that the person pressed for a different
            reason must not connect a computer. */}
        <button
          type="submit"
          name="accept"
          id="kc-login"
          value="true"
          className={buttonClass({ variant: "primary", className: "px-6" })}
        >
          {msgStr(app ? "kdsConnect" : "kdsGrantAllow")}
        </button>
      </form>
    </Stage>
  );
}
