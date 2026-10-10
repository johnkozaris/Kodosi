import type { ExtendKcContext } from "keycloakify/login";
import type { KcEnvName, ThemeName } from "../kc.gen";

type KcContextExtension = {
  themeName: ThemeName;
  properties: Record<KcEnvName, string>;
};

type KcContextExtensionPerPage = {};

export type KcContext = ExtendKcContext<KcContextExtension, KcContextExtensionPerPage>;

export type PageOf<Id extends KcContext["pageId"]> = Extract<KcContext, { pageId: Id }>;
