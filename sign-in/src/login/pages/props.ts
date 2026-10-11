import type { I18n } from "../i18n";
import type { KcContext, PageOf } from "../KcContext";

export type Page<Id extends KcContext["pageId"]> = { kcContext: PageOf<Id>; i18n: I18n };
