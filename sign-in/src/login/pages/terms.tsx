import { kcSanitize } from "keycloakify/lib/kcSanitize";

import { buttonClass } from "../../parts/button";
import { cn } from "../../parts/cn";
import { Stage } from "../stage";
import type { Page } from "./props";

/** The terms of a realm. The words are the realm's own. */
export default function Terms({ kcContext, i18n }: Page<"terms.ftl">) {
  const { url } = kcContext;
  const { msgStr } = i18n;
  const text = msgStr("termsText");
  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={msgStr("kdsTermsTitle")}
      quiet={kcContext.message?.type === "warning"}
      wide
    >
      {/* A realm that wrote no terms yet shows the two answers alone. */}
      {text.trim() && (
        <div
          className="prose-terms max-h-[42vh] overflow-y-auto px-5 py-5 text-[14.5px]"
          dangerouslySetInnerHTML={{ __html: kcSanitize(text) }}
        />
      )}
      <form
        action={url.loginAction}
        method="post"
        className={cn(
          "relative flex justify-end gap-2 p-3",
          text.trim() &&
            "before:absolute before:top-0 before:right-0 before:left-[18px] before:h-px before:bg-hairline/60",
        )}
      >
        <button
          type="submit"
          name="cancel"
          id="kc-decline"
          value="true"
          className={buttonClass({ variant: "ghost" })}
        >
          {msgStr("kdsDecline")}
        </button>
        <button
          type="submit"
          name="accept"
          id="kc-accept"
          value="true"
          className={buttonClass({ variant: "primary", className: "px-6" })}
        >
          {msgStr("kdsAccept")}
        </button>
      </form>
    </Stage>
  );
}
