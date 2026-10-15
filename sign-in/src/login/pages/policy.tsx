import type { PasswordPolicies } from "keycloakify/login/KcContext";

import { cn } from "../../parts/cn";
import type { I18n } from "../i18n";

interface Rule {
  text: string;
  met: boolean;
  not?: boolean;
}

function rules(
  i18n: I18n,
  policies: PasswordPolicies,
  password: string,
  person: { username?: string | undefined; email?: string | undefined },
): Rule[] {
  const { msgStr } = i18n;
  const count = (letters: RegExp) => (password.match(letters) ?? []).length;
  const letters = [...password].length;
  const found: Rule[] = [];
  if (policies.length)
    found.push({
      text: msgStr("kdsPolicyLength", `${policies.length}`),
      met: letters >= policies.length,
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
  if (policies.maxLength)
    found.push({
      text: msgStr("kdsPolicyMax", `${policies.maxLength}`),
      met: letters <= policies.maxLength,
      not: true,
    });
  if (policies.notUsername && person.username)
    found.push({
      text: msgStr("kdsPolicyNotUsername"),
      met: password.toLowerCase() !== person.username.toLowerCase(),
      not: true,
    });
  if (policies.notEmail && person.email)
    found.push({
      text: msgStr("kdsPolicyNotEmail"),
      met: password.toLowerCase() !== person.email.trim().toLowerCase(),
      not: true,
    });
  return found;
}

export function meetsPolicy(
  i18n: I18n,
  policies: PasswordPolicies | undefined,
  password: string,
  person: { username?: string | undefined; email?: string | undefined } = {},
): boolean {
  return !policies || rules(i18n, policies, password, person).every((rule) => rule.met);
}

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
  const found = (policies ? rules(i18n, policies, password, { username, email }) : []).filter(
    (rule) => !rule.not || (password && !rule.met),
  );
  if (!found.length) return null;
  return (
    <ul className="flex flex-wrap gap-1.5 px-[18px] pt-1 pb-3.5">
      {found.map(({ text, met, not }) => (
        <li
          key={text}
          className={cn(
            "rounded-full px-2.5 py-[3px] text-caption font-medium transition-colors duration-200",
            not
              ? "pop bg-danger-soft text-danger"
              : password && met
                ? "bg-ready-soft text-ready"
                : "bg-well text-ink-muted",
          )}
        >
          {text}
          {password && met && !not && <span className="sr-only">, {i18n.msgStr("kdsDone")}</span>}
        </li>
      ))}
    </ul>
  );
}
