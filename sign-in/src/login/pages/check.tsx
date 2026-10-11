import { useEffect, useRef, useState } from "react";

import { buttonClass } from "../../parts/button";
import { PersonCheckRow, REFUSALS, usePersonCheck } from "../../parts/captcha";
import { cn } from "../../parts/cn";
import { rule } from "../../parts/rows";
import { QuietLink } from "../../parts/text";
import { isMessage } from "../i18n";
import { Stage } from "../stage";
import type { Page } from "./props";

/**
 * The check that a person, not a program, goes on, when the realm gives it a page of its own
 * before a form. For most people it passes by itself, and the page then goes on by itself.
 */
export function CheckPage({
  kcContext,
  i18n,
}: Page<"turnstile-form.ftl" | "turnstile-registration-form.ftl">) {
  const { url, message } = kcContext;
  const { msgStr } = i18n;
  const form = "kc-turnstile-form";
  const check = usePersonCheck(kcContext, form);
  const [answered, setAnswered] = useState(false);
  const body = useRef<HTMLFormElement>(null);
  const reset = "isResetFlow" in kcContext && !!kcContext.isResetFlow;
  const joins =
    kcContext.pageId === "turnstile-registration-form.ftl" ||
    ("isRegistrationFlow" in kcContext && !!kcContext.isRegistrationFlow);
  const refused =
    message?.type === "error" && REFUSALS.some((key) => isMessage(i18n, message.summary, key));

  useEffect(() => {
    if (check.passed && check.check) body.current?.requestSubmit();
  }, [check.passed, check.check]);

  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr(reset ? "kdsForgotTitle" : joins ? "kdsJoinTitle" : "kdsSignIn")}
      lead={msgStr("kdsPersonCheckLead")}
      wrong={refused && !answered}
      quiet={refused}
      waits={!check.passed && !check.failed}
      below={<QuietLink href={url.loginUrl}>{msgStr("kdsBackToSignIn")}</QuietLink>}
    >
      <form ref={body} id={form} action={url.loginAction} method="post" onSubmit={check.hold}>
        <PersonCheckRow
          check={check.check}
          label={msgStr("kdsNotRobot")}
          problem={
            check.failed
              ? msgStr("kdsCheckFailed")
              : refused && !answered
                ? message.summary
                : undefined
          }
          againLabel={msgStr("kdsCheckAgain")}
          onAgain={() => {
            setAnswered(true);
            check.again();
          }}
        />
        <div className={cn(rule, "p-3")}>
          <button
            type="submit"
            name="login"
            id="kc-login"
            value="true"
            className={buttonClass({ variant: "primary", size: "lg", className: "w-full" })}
          >
            {msgStr("kdsContinue")}
          </button>
        </div>
      </form>
    </Stage>
  );
}
