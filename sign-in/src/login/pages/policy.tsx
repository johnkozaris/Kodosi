import type { PasswordPolicies } from "keycloakify/login/KcContext";

import { cn } from "../../parts/cn";
import type { I18n } from "../i18n";

/** What the realm asks of a password, as the page can check it while the person types. */
function rules(
  i18n: I18n,
  policies: PasswordPolicies,
  password: string,
  person: { username?: string | undefined; email?: string | undefined },
): { text: string; met: boolean }[] {
  const { msgStr } = i18n;
  const count = (letters: RegExp) => (password.match(letters) ?? []).length;
  const found: { text: string; met: boolean }[] = [];
  if (policies.length)
    found.push({
      text: msgStr("kdsPolicyLength", `${policies.length}`),
      met: [...password].length >= policies.length,
    });
  if (policies.digits)
    found.push({ text: msgStr("kdsPolicyDigits"), met: count(/\d/g) >= policies.digits });
  if (policies.lowerCase)
    found.push({ text: msgStr("kdsPolicyLower"), met: count(/\p{Ll}/gu) >= policies.lowerCase });
  if (policies.upperCase)
    found.push({ text: msgStr("kdsPolicyUpper"), met: count(/\p{Lu}/gu) >= policies.upperCase });
  if (policies.specialChars)
    found.push({
      text: msgStr("kdsPolicySpecial"),
      met: count(/[^\p{L}\p{N}\s]/gu) >= policies.specialChars,
    });
  if (policies.notUsername && person.username)
    found.push({ text: msgStr("kdsPolicyNotUsername"), met: password !== person.username });
  if (policies.notEmail && person.email)
    found.push({ text: msgStr("kdsPolicyNotEmail"), met: password !== person.email });
  return found;
}

/**
 * What the realm asks of a password: one small tag for each rule. A tag turns green when the
 * password has it, so the person knows before they send. Keycloak still does the real check.
 */
export function PolicyTags({
  i18n,
  policies,
  password,
  username,
  email,
}: {
  i18n: I18n;
  policies: PasswordPolicies | undefined;
  password: string;
  username?: string | undefined;
  email?: string | undefined;
}) {
  const found = policies ? rules(i18n, policies, password, { username, email }) : [];
  if (!found.length) return null;
  return (
    <ul className="flex flex-wrap gap-1.5 px-[18px] pt-1 pb-3.5">
      {found.map(({ text, met }) => (
        <li
          key={text}
          className={cn(
            "rounded-full px-2.5 py-[3px] text-caption font-medium transition-colors duration-200",
            password && met ? "bg-ready-soft text-ready" : "bg-well text-ink-muted",
          )}
        >
          {text}
          {password && met && <span className="sr-only">, {i18n.msgStr("kdsDone")}</span>}
        </li>
      ))}
    </ul>
  );
}
