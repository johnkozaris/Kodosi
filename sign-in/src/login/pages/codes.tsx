import { Check, Copy, Download } from "lucide-react";
import { useState } from "react";

import { buttonClass } from "../../parts/button";
import { cn } from "../../parts/cn";
import { CodeCells } from "../../parts/code";
import { option, Pill } from "../../parts/pill";
import { ActionRow, Go, SignOutOthers, SwitchRow, TextRow } from "../../parts/rows";
import { QuietLink, quietLink } from "../../parts/text";
import { IconTile } from "../../parts/tile";
import { Stage } from "../stage";
import type { Page } from "./props";

/** The digits of a code from an authenticator app, when the page does not know the realm's choice. */
const DIGITS = 6;

/** The code of the authenticator app. A person with more than one app chooses which. */
export function Otp({ kcContext, i18n }: Page<"login-otp.ftl">) {
  const { otpLogin, url, messagesPerField } = kcContext;
  const { msgStr } = i18n;
  const apps = otpLogin.userOtpCredentials;
  const [app, setApp] = useState(otpLogin.selectedCredentialId ?? apps[0]?.id);
  const wrong = messagesPerField.existsError("totp");
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsCodeTitle")}
      lead={msgStr("kdsCodeLead")}
      wrong={wrong}
      problem={wrong ? messagesPerField.get("totp") : undefined}
      quiet={wrong}
    >
      <form id="kc-otp-login-form" action={url.loginAction} method="post" noValidate>
        {apps.length > 1 && (
          <div className="flex justify-center px-4 pt-4">
            <Pill>
              {apps.map((one) => (
                <label
                  key={one.id}
                  data-on={one.id === app ? "" : undefined}
                  className={cn(option(one.id === app), "cursor-pointer")}
                >
                  <input
                    type="radio"
                    name="selectedCredentialId"
                    value={one.id}
                    checked={one.id === app}
                    onChange={() => setApp(one.id)}
                    className="sr-only"
                  />
                  {one.userLabel}
                </label>
              ))}
            </Pill>
          </div>
        )}
        <CodeCells
          id="otp"
          name="otp"
          length={DIGITS}
          label={msgStr("kdsCode")}
          invalid={wrong}
          autoFocus
        />
        <input type="hidden" name="login" value="true" />
      </form>
    </Stage>
  );
}

/** One code of the list that the person kept. */
export function RecoveryInput({ kcContext, i18n }: Page<"login-recovery-authn-code-input.ftl">) {
  const { url, messagesPerField, recoveryAuthnCodesInputBean } = kcContext;
  const { msgStr } = i18n;
  const wrong = messagesPerField.existsError("recoveryCodeInput");
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsRecoveryCodeTitle")}
      lead={msgStr("kdsRecoveryCodeLead", `${recoveryAuthnCodesInputBean.codeNumber}`)}
      wrong={wrong}
      problem={wrong ? messagesPerField.get("recoveryCodeInput") : undefined}
      quiet={wrong}
    >
      <form id="kc-recovery-code-login-form" action={url.loginAction} method="post" noValidate>
        <TextRow
          id="recoveryCodeInput"
          name="recoveryCodeInput"
          label={msgStr("kdsRecoveryCode")}
          autoFocus
          autoComplete="off"
          autoCapitalize="none"
          spellCheck={false}
          required
          aria-invalid={wrong}
          end={<Go label={msgStr("kdsSignIn")} name="login" id="kc-login" />}
        />
      </form>
    </Stage>
  );
}

/** A new authenticator app: the picture to scan, the first code, and a name for the phone. */
export function ConfigTotp({ kcContext, i18n }: Page<"login-config-totp.ftl">) {
  const { url, totp, mode, messagesPerField, isAppInitiatedAction } = kcContext;
  const { msgStr } = i18n;
  const wrong = messagesPerField.existsError("totp", "userLabel");
  const named = totp.otpCredentials.length >= 1;
  const form = "kc-totp-settings-form";
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsAuthenticatorTitle")}
      lead={msgStr("kdsAuthenticatorLead")}
      wrong={wrong}
      problem={wrong ? messagesPerField.getFirstError("totp", "userLabel") : undefined}
      quiet={wrong || kcContext.message?.type === "warning"}
      below={
        isAppInitiatedAction && (
          <button type="submit" form={form} name="cancel-aia" value="true" className={quietLink}>
            {msgStr("kdsCancel")}
          </button>
        )
      }
    >
      <div className="flex flex-col items-center gap-3 px-5 pt-6 pb-5 text-center">
        {mode === "manual" ? (
          <>
            <p className="text-footnote text-ink-muted">{msgStr("kdsTypeKey")}</p>
            <code className="well rounded-md px-3.5 py-2.5 font-mono text-callout tracking-[0.06em] text-balance text-ink select-all">
              {totp.totpSecretEncoded}
            </code>
            <QuietLink href={totp.qrUrl}>{msgStr("kdsScanInstead")}</QuietLink>
          </>
        ) : (
          <>
            {/* The picture stays dark on white in each theme: a phone reads it. */}
            <span className="rounded-lg bg-white p-2.5 shadow-well">
              <img
                src={`data:image/png;base64, ${totp.totpSecretQrCode}`}
                alt={msgStr("kdsScan")}
                className="block size-40"
              />
            </span>
            <p className="text-footnote text-ink-muted">{msgStr("kdsScan")}</p>
            <QuietLink href={totp.manualUrl}>{msgStr("kdsCannotScan")}</QuietLink>
          </>
        )}
      </div>
      <form id={form} action={url.loginAction} method="post" noValidate>
        <div className="relative before:absolute before:top-0 before:right-0 before:left-[18px] before:h-px before:bg-hairline/60">
          <p className="px-5 pt-4 text-center text-footnote text-ink-muted">
            {msgStr("kdsThenCode")}
          </p>
          <CodeCells
            id="totp"
            name="totp"
            length={totp.policy.digits}
            label={msgStr("kdsCode")}
            invalid={messagesPerField.existsError("totp")}
            send={false}
          />
        </div>
        <TextRow
          id="userLabel"
          name="userLabel"
          label={msgStr("kdsPhoneName")}
          autoComplete="off"
          required={named}
          aria-invalid={messagesPerField.existsError("userLabel")}
          end={<Go label={msgStr("kdsFinish")} id="saveTOTPBtn" />}
        />
        <SignOutOthers shown={!!isAppInitiatedAction} label={msgStr("kdsSignOutOthers")} />
        <input type="hidden" id="totpSecret" name="totpSecret" value={totp.totpSecret} />
        {mode && <input type="hidden" id="mode" name="mode" value={mode} />}
      </form>
    </Stage>
  );
}

/** The codes for a day without the phone: the person keeps them before the step ends. */
export function RecoveryConfig({ kcContext, i18n }: Page<"login-recovery-authn-code-config.ftl">) {
  const { url, recoveryAuthnCodesConfigBean: codes, isAppInitiatedAction } = kcContext;
  const { msgStr } = i18n;
  const [kept, setKept] = useState(false);
  const [copied, setCopied] = useState(false);
  const list = codes.generatedRecoveryAuthnCodesList;
  const text = list.map((code, i) => `${i + 1}: ${code}`).join("\n");
  const form = "kc-recovery-codes-settings-form";
  const action = buttonClass({ variant: "ghost", size: "sm" });

  const copy = () => {
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    });
  };
  const download = () => {
    const link = document.createElement("a");
    link.href = URL.createObjectURL(new Blob([`${text}\n`], { type: "text/plain" }));
    link.download = "kodosi-recovery-codes.txt";
    link.click();
    URL.revokeObjectURL(link.href);
  };

  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsRecoveryTitle")}
      lead={msgStr("kdsRecoveryLead")}
      quiet={kcContext.message?.type === "warning"}
      below={
        isAppInitiatedAction && (
          <button type="submit" form={form} name="cancel-aia" value="true" className={quietLink}>
            {msgStr("kdsCancel")}
          </button>
        )
      }
    >
      <ol className="grid grid-cols-2 gap-x-2.5 gap-y-1.5 px-4 pt-5 pb-2">
        {list.map((code, i) => (
          <li key={code} className="well flex items-baseline gap-2 rounded-sm px-3 py-1.5">
            <span className="w-4 text-right text-caption text-ink-muted tabular-nums">{i + 1}</span>
            <span className="font-mono text-footnote text-ink">{code}</span>
          </li>
        ))}
      </ol>
      <div className="flex justify-center gap-1 px-4 pb-3">
        <button type="button" onClick={copy} className={action}>
          {copied ? <Check aria-hidden /> : <Copy aria-hidden />}
          {msgStr(copied ? "kdsCopied" : "kdsCopy")}
        </button>
        <button type="button" onClick={download} className={action}>
          <Download aria-hidden />
          {msgStr("kdsDownload")}
        </button>
      </div>
      <form id={form} action={url.loginAction} method="post">
        <input
          type="hidden"
          name="generatedRecoveryAuthnCodes"
          value={codes.generatedRecoveryAuthnCodesAsString}
        />
        <input type="hidden" name="generatedAt" value={codes.generatedAt} />
        <input
          type="hidden"
          id="userLabel"
          name="userLabel"
          value={msgStr("recovery-codes-label-default")}
        />
        <SwitchRow
          id="kcRecoveryCodesConfirmationCheck"
          name="kcRecoveryCodesConfirmationCheck"
          label={msgStr("kdsSavedCodes")}
          checked={kept}
          onChange={(event) => setKept(event.target.checked)}
        />
        <SignOutOthers shown={!!isAppInitiatedAction} label={msgStr("kdsSignOutOthers")} />
        <ActionRow
          id="saveRecoveryAuthnCodesBtn"
          disabled={!kept}
          tile={<IconTile tint="green" icon={<Check />} />}
          title={msgStr("kdsDone")}
        />
      </form>
    </Stage>
  );
}
