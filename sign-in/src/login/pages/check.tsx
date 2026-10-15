import { useEffect, useRef } from "react";

import { buttonClass } from "../../parts/button";
import { cn } from "../../parts/cn";
import { rule } from "../../parts/rows";
import { QuietLink } from "../../parts/text";
import { PersonCheckRow, usePersonCheck } from "../captcha";
import { Stage } from "../stage";
import type { Page } from "./props";

export function CheckPage({
  kcContext,
  i18n,
}: Page<"turnstile-form.ftl" | "turnstile-registration-form.ftl">) {
  const { url } = kcContext;
  const { msgStr } = i18n;
  const form = "kc-turnstile-form";
  const check = usePersonCheck(kcContext, form, i18n);
  const body = useRef<HTMLFormElement>(null);
  const reset = "isResetFlow" in kcContext && !!kcContext.isResetFlow;
  const joins =
    kcContext.pageId === "turnstile-registration-form.ftl" ||
    ("isRegistrationFlow" in kcContext && !!kcContext.isRegistrationFlow);

  useEffect(() => {
    if (check.passed && check.config) body.current?.requestSubmit();
  }, [check.passed, check.config]);

  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr(reset ? "kdsForgotTitle" : joins ? "kdsJoinTitle" : "kdsSignIn")}
      lead={msgStr("kdsPersonCheckLead")}
      wrong={check.refused && !check.retried}
      quiet={check.refused}
      waits={!check.passed && !check.failed}
      below={<QuietLink href={url.loginUrl}>{msgStr("kdsBackToSignIn")}</QuietLink>}
    >
      <form ref={body} id={form} action={url.loginAction} method="post" onSubmit={check.hold}>
        <PersonCheckRow check={check} i18n={i18n} />
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
