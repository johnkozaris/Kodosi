import type { I18n } from "../i18n";
import type { KcContext } from "../KcContext";

export type Page<Id extends KcContext["pageId"]> = {
  kcContext: Extract<KcContext, { pageId: Id }>;
  i18n: I18n;
};
