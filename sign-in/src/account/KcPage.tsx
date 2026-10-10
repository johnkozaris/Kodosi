import { useCallback } from "react";

import { Account } from "./account";
import type { KcContext } from "./KcContext";
import { openAccount } from "./keycloak";

/** Keycloak's account pages are one page here. */
export default function KcPage({ kcContext }: { kcContext: KcContext }) {
  const open = useCallback(() => openAccount(kcContext), [kcContext]);
  return <Account kcContext={kcContext} open={open} />;
}
