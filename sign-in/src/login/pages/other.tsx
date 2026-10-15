import { i18nBuilder } from "keycloakify/login";
import DefaultPage from "keycloakify/login/DefaultPage";
import type { TemplateProps } from "keycloakify/login/TemplateProps";
import UserProfileFormFields from "keycloakify/login/UserProfileFormFields";
import { createContext, use } from "react";

import type { I18n } from "../i18n";
import type { KcContext } from "../KcContext";
import { Stage } from "../stage";

const { useI18n: useOwnWords, ofTypeI18n } = i18nBuilder.build();
type OwnWords = typeof ofTypeI18n;

const Words = createContext<I18n | null>(null);

function Frame({
  kcContext,
  headerNode,
  infoNode,
  socialProvidersNode,
  documentTitle,
  displayMessage = true,
  displayInfo,
  children,
}: TemplateProps<KcContext, OwnWords>) {
  const words = use(Words);
  if (!words) return null;
  return (
    <Stage
      kcContext={kcContext}
      i18n={words}
      {...(documentTitle ? { tab: documentTitle } : {})}
      title={headerNode}
      quiet={!displayMessage}
      wide
      below={
        (socialProvidersNode || (displayInfo && infoNode)) && (
          <div className="plain-page w-full text-center">
            {socialProvidersNode}
            {displayInfo && infoNode}
          </div>
        )
      }
    >
      <div className="plain-page px-5 py-5">{children}</div>
    </Stage>
  );
}

export default function Other({ kcContext, i18n }: { kcContext: KcContext; i18n: I18n }) {
  const { i18n: own } = useOwnWords({ kcContext });
  if (
    kcContext.pageId === "turnstile-form.ftl" ||
    kcContext.pageId === "turnstile-registration-form.ftl"
  )
    return null;
  return (
    <Words value={i18n}>
      <DefaultPage
        kcContext={kcContext}
        i18n={own}
        classes={{}}
        Template={Frame}
        doUseDefaultCss={false}
        UserProfileFormFields={UserProfileFormFields}
        doMakeUserConfirmPassword
      />
    </Words>
  );
}
