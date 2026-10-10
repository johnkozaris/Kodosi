import { kcSanitize } from "keycloakify/lib/kcSanitize";
import type { Attribute, PasswordPolicies } from "keycloakify/login/KcContext";
import {
  type KeyboardEvent,
  lazy,
  type ReactNode,
  Suspense,
  useEffect,
  useRef,
  useState,
} from "react";

import { buttonClass } from "../../parts/button";
import { askOnPress, PersonCheckRow, usePersonCheck } from "../../parts/captcha";
import { cn } from "../../parts/cn";
import { Avatar } from "../../parts/person";
import { Go, PasswordRow, rule, SwitchRow, TextRow } from "../../parts/rows";
import { QuietLink, quietLink } from "../../parts/text";
import type { I18n } from "../i18n";
import { Stage } from "../stage";
import { PolicyTags } from "./policy";
import type { Page } from "./props";

// A profile with fields that a row cannot hold takes Keycloak's own form, which loads then.
const Other = lazy(() => import("./other"));

type Joins = Page<
  "register.ftl" | "login-update-profile.ftl" | "idp-review-user-profile.ftl" | "update-email.ftl"
>;

/** The fields that a row of the capsule can hold. */
const PLAIN = new Set([undefined, "text", "html5-email", "html5-tel", "html5-url"]);

function plainProfile(kcContext: Joins["kcContext"]): boolean {
  return Object.values(kcContext.profile.attributesByName).every(
    (attribute) =>
      !attribute.multivalued &&
      !attribute.annotations.inputOptionsFromValidation &&
      PLAIN.has(attribute.annotations.inputType),
  );
}

function labelOf(i18n: I18n, attribute: Attribute): string {
  const { msgStr, advancedMsgStr } = i18n;
  switch (attribute.name) {
    case "firstName":
      return msgStr("kdsFirstName");
    case "lastName":
      return msgStr("kdsLastName");
    case "email":
      return msgStr("kdsEmail");
    case "username":
      return msgStr("kdsUsername");
    default:
      return advancedMsgStr(attribute.displayName ?? attribute.name);
  }
}

/** Keycloak keeps a username in small letters, so the page shows it so while the person types. */
function asHandle(typed: string): string {
  return typed.replace(/^@+/, "").replace(/\s+/g, "").toLowerCase();
}

/** What is wrong with a value, as far as the page can know before Keycloak checks it. */
function problemOf(i18n: I18n, attribute: Attribute, value: string): string | null {
  const { msgStr, advancedMsgStr } = i18n;
  const typed = value.trim();
  if (!typed) return attribute.required ? msgStr("kdsFill") : null;
  const { length, pattern, email } = attribute.validators;
  if ((email || attribute.name === "email") && !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(typed))
    return msgStr("kdsBadEmail");
  const min = Number(length?.min ?? 0);
  const max = Number(length?.max ?? 0);
  const letters = [...typed].length;
  if (letters < min || (max > 0 && letters > max))
    return max > 0 ? msgStr("kdsBadLength", `${min}`, `${max}`) : msgStr("kdsTooShort", `${min}`);
  if (pattern?.pattern) {
    let fits = true;
    try {
      fits = new RegExp(pattern.pattern).test(typed);
    } catch {
      // Keycloak reads the pattern in its own way: it does the check.
    }
    if (!fits)
      return pattern["error-message"]
        ? advancedMsgStr(pattern["error-message"])
        : msgStr("kdsBadCharacters");
  }
  return null;
}

type StepId = "handle" | "details" | "password";

/** The person as their friends see them from now on: their circle, their name, their username.
    It is the way back to the step that made it. */
function PersonChip({
  name,
  handle,
  label,
  onPress,
}: {
  name: string;
  handle: string;
  label: string;
  onPress: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onPress}
      aria-label={`${label}: ${[name, handle && `@${handle}`].filter(Boolean).join(", ")}`}
      className="sheet rise inline-flex h-10 max-w-full items-center gap-2.5 rounded-full pr-4 pl-1.5 text-footnote transition-[background-color,scale] duration-200 hover:bg-lifted active:scale-[0.97]"
    >
      <Avatar name={name || handle} size={28} />
      {name && <span className="truncate font-medium text-ink">{name}</span>}
      {handle && <span className="truncate font-mono text-ink-muted">@{handle}</span>}
    </button>
  );
}

/**
 * A new account, or the details of a first sign-in. It is one form of Keycloak, and the person
 * meets it one thing at a time: the username, then the details, then the password. The username
 * is the first moment, because it is the name that friends find a person by. Each field of
 * Keycloak's own page is in the form from the start, with its own name, so a password manager
 * and Keycloak see the form that they know.
 */
function Joining({
  kcContext,
  i18n,
  form,
  action,
  finish,
  detailsTitle,
  password,
  extra,
  below,
}: Joins & {
  /** The id of Keycloak's own form. */
  form: string;
  action: string;
  /** The words of the last button. */
  finish: string;
  detailsTitle: string;
  /** The form makes a password, with the rules of the realm. */
  password?: { policies: PasswordPolicies | undefined };
  /** More rows at the end of the details. */
  extra?: ReactNode;
  below?: ReactNode;
}) {
  const { profile, messagesPerField, isAppInitiatedAction } = kcContext;
  const { msgStr } = i18n;
  const attributes = Object.values(profile.attributesByName);
  const handleAttribute = attributes.find((one) => one.name === "username" && !one.readOnly);
  const details = attributes.filter((one) => one !== handleAttribute);
  const terms = "termsAcceptanceRequired" in kcContext && !!kcContext.termsAcceptanceRequired;
  const steps: StepId[] = [
    ...(handleAttribute ? (["handle"] as const) : []),
    ...(details.length ? (["details"] as const) : []),
    ...(password ? (["password"] as const) : []),
  ];
  const last = steps[steps.length - 1];

  const [values, setValues] = useState<Record<string, string>>(() =>
    Object.fromEntries(attributes.map((one) => [one.name, one.value ?? ""])),
  );
  const [secret, setSecret] = useState("");
  const [accepted, setAccepted] = useState(false);
  const [reads, setReads] = useState(false);
  const again = useRef<HTMLInputElement>(null);
  const { check, held, hold } = usePersonCheck(kcContext, form);

  // Keycloak refused the form: the page opens at the step of the first field that it names.
  const refusedAt = attributes.find((one) => messagesPerField.existsError(one.name));
  const secretRefused = messagesPerField.existsError("password", "password-confirm");
  const termsRefused = messagesPerField.existsError("termsAccepted");
  const [step, setStep] = useState<StepId>(() => {
    if (refusedAt) return refusedAt === handleAttribute ? "handle" : "details";
    if (secretRefused || termsRefused || kcContext.message?.type === "error") return last as StepId;
    return steps[0] as StepId;
  });
  // What the page itself found wrong, at a press of "Next".
  const [found, setFound] = useState<{ field: string; text: string; attempt: number } | null>(null);
  const moved = useRef(false);
  const body = useRef<HTMLFormElement>(null);

  useEffect(() => {
    // The step that the person goes to takes the typing. The first step has it from the page.
    if (!moved.current) return;
    const first = body.current?.querySelector<HTMLInputElement>(
      `[data-step="${step}"] input:not([type=hidden],[type=checkbox])`,
    );
    first?.focus();
  }, [step]);

  const go = (to: StepId) => {
    moved.current = true;
    setFound(null);
    setStep(to);
  };
  const refuse = (field: string, text: string) => {
    setFound((before) => ({ field, text, attempt: (before?.attempt ?? 0) + 1 }));
    body.current?.querySelector<HTMLInputElement>(`[name="${CSS.escape(field)}"]`)?.focus();
    return false;
  };
  /** Checks the fields of the step that the person leaves. */
  const sound = (): boolean => {
    const fields =
      step === "handle" ? [handleAttribute as Attribute] : step === "details" ? details : [];
    for (const attribute of fields) {
      const text = problemOf(i18n, attribute, values[attribute.name] ?? "");
      if (text) return refuse(attribute.name, text);
    }
    if (step === "password" && !secret) return refuse("password", msgStr("kdsFill"));
    return true;
  };
  const next = () => {
    if (sound()) go(steps[steps.indexOf(step) + 1] as StepId);
  };
  /** Enter in a field of a step goes to the next step. In the last step it sends the form. */
  const enter = (event: KeyboardEvent) => {
    if (event.key !== "Enter" || step === last || event.nativeEvent.isComposing) return;
    event.preventDefault();
    next();
  };
  const change = (name: string, value: string) => setValues((now) => ({ ...now, [name]: value }));

  const handle = values.username ?? "";
  const name = [values.firstName, values.lastName].filter((part) => part?.trim()).join(" ");
  const serverProblem = refusedAt
    ? messagesPerField.get(refusedAt.name)
    : secretRefused
      ? messagesPerField.getFirstError("password", "password-confirm")
      : termsRefused
        ? messagesPerField.get("termsAccepted")
        : undefined;
  const wrongField = found?.field ?? refusedAt?.name ?? (secretRefused ? "password" : undefined);
  const title =
    step === "handle"
      ? msgStr("kdsHandleTitle")
      : step === "details"
        ? detailsTitle
        : msgStr("kdsPasswordTitle");
  const termsText = terms ? msgStr("termsText") : "";

  const finishButton = (
    <div className={cn(rule, "p-3")}>
      <button
        type="submit"
        disabled={terms && !accepted}
        {...askOnPress(check)}
        className={buttonClass({
          variant: "primary",
          size: "lg",
          className: cn("w-full", check && !check.widget && "g-recaptcha"),
        })}
      >
        {finish}
      </button>
    </div>
  );

  return (
    <Stage
      kcContext={kcContext}
      i18n={i18n}
      title={title}
      lead={
        step === "handle"
          ? msgStr("kdsHandleLead")
          : (name || handle) && (
              <PersonChip
                name={name}
                handle={handle}
                label={msgStr("kdsBack")}
                onPress={() => go(steps[0] as StepId)}
              />
            )
      }
      wrong={!!found || !!serverProblem}
      problem={found?.text ?? serverProblem}
      attempt={found?.attempt ?? 0}
      quiet={!!serverProblem || kcContext.message?.type === "warning"}
      waits={held}
      below={
        <>
          {step !== steps[0] && (
            <button
              type="button"
              className={quietLink}
              onClick={() => go(steps[steps.indexOf(step) - 1] as StepId)}
            >
              {msgStr("kdsBack")}
            </button>
          )}
          {below}
          {isAppInitiatedAction && (
            <button type="submit" form={form} name="cancel-aia" value="true" className={quietLink}>
              {msgStr("kdsCancel")}
            </button>
          )}
        </>
      }
    >
      <form
        ref={body}
        id={form}
        action={action}
        method="post"
        noValidate
        onSubmit={(event) => {
          // A press of Enter in a step that is not the last is a "Next", and a form with a
          // field that the page knows to be wrong does not go.
          if (step !== last || !sound()) {
            event.preventDefault();
            if (step !== last) next();
            return;
          }
          if (hold(event)) return;
          if (again.current) again.current.value = secret;
        }}
      >
        {handleAttribute && (
          <div data-step="handle" className={step === "handle" ? "resolve" : "hidden"}>
            <div className="flex items-center gap-3 py-4 pr-2.5 pl-4">
              <Avatar name={handle} size={44} />
              <label className="flex min-w-0 flex-1 items-baseline gap-px">
                <span
                  aria-hidden
                  className="font-mono text-[21px] leading-[30px] font-medium text-ink-muted"
                >
                  @
                </span>
                <input
                  id="username"
                  name="username"
                  className="handle-input"
                  aria-label={msgStr("kdsUsername")}
                  aria-invalid={wrongField === "username" || undefined}
                  placeholder={msgStr("kdsHandleSample")}
                  value={handle}
                  onChange={(event) => change("username", asHandle(event.target.value))}
                  onKeyDown={enter}
                  autoFocus={step === "handle"}
                  autoComplete="username"
                  autoCapitalize="none"
                  autoCorrect="off"
                  spellCheck={false}
                  required
                  maxLength={Number(handleAttribute.validators.length?.max ?? 0) || undefined}
                />
              </label>
              {last === "handle" ? null : (
                <Go type="button" label={msgStr("kdsNext")} onClick={next} />
              )}
            </div>
            {last === "handle" && finishButton}
          </div>
        )}
        {details.length > 0 && (
          <div data-step="details" className={step === "details" ? "resolve" : "hidden"}>
            {details.map((attribute, i) => (
              <TextRow
                key={attribute.name}
                id={attribute.name}
                name={attribute.name}
                label={labelOf(i18n, attribute)}
                type={attribute.annotations.inputType?.replace("html5-", "") ?? "text"}
                value={values[attribute.name] ?? ""}
                onChange={(event) => change(attribute.name, event.target.value)}
                onKeyDown={enter}
                readOnly={attribute.readOnly}
                required={attribute.required}
                autoComplete={attribute.autocomplete}
                autoFocus={step === "details" && i === 0}
                aria-invalid={wrongField === attribute.name || undefined}
                end={
                  last !== "details" && i === details.length - 1 ? (
                    <Go type="button" label={msgStr("kdsNext")} onClick={next} />
                  ) : undefined
                }
              />
            ))}
            {extra}
            {last === "details" && finishButton}
          </div>
        )}
        {password && (
          <div data-step="password" className={step === "password" ? "resolve" : "hidden"}>
            <PasswordRow
              id="password"
              name="password"
              label={msgStr("kdsPassword")}
              autoComplete="new-password"
              autoFocus={step === "password"}
              required
              aria-invalid={wrongField === "password" || undefined}
              value={secret}
              onChange={(event) => setSecret(event.target.value)}
              show={msgStr("kdsShowPassword")}
              hide={msgStr("kdsHidePassword")}
              capsLock={msgStr("kdsCapsLock")}
            />
            {/* Keycloak asks for the password two times. The person types it one time and can
                look at it, so the form sends the same value in the second field. */}
            <input ref={again} type="hidden" id="password-confirm" name="password-confirm" />
            <PolicyTags
              i18n={i18n}
              policies={password.policies}
              password={secret}
              username={handle}
              email={values.email}
            />
            {terms && (
              <>
                <SwitchRow
                  id="termsAccepted"
                  name="termsAccepted"
                  label={
                    <>
                      {msgStr("kdsAcceptTerms")}
                      {termsText.trim() && (
                        <button
                          type="button"
                          aria-expanded={reads}
                          aria-controls="kc-registration-terms-text"
                          onClick={(event) => {
                            event.preventDefault();
                            setReads((now) => !now);
                          }}
                          className={cn(quietLink, "ml-2")}
                        >
                          {msgStr("kdsReadTerms")}
                        </button>
                      )}
                    </>
                  }
                  checked={accepted}
                  onChange={(event) => setAccepted(event.target.checked)}
                  aria-invalid={termsRefused || undefined}
                />
                {reads && (
                  <div
                    id="kc-registration-terms-text"
                    className={cn(
                      rule,
                      "prose-terms resolve max-h-[36vh] overflow-y-auto px-[18px] py-4 text-[14px]",
                    )}
                    dangerouslySetInnerHTML={{ __html: kcSanitize(termsText) }}
                  />
                )}
              </>
            )}
            <PersonCheckRow check={check} label={msgStr("kdsNotRobot")} />
            {finishButton}
          </div>
        )}
      </form>
    </Stage>
  );
}

/** A person makes their account. */
export function Register(props: Page<"register.ftl">) {
  const { kcContext, i18n } = props;
  const { msgStr } = i18n;
  if (!plainProfile(kcContext)) {
    return (
      <Suspense>
        <Other {...props} />
      </Suspense>
    );
  }
  return (
    <Joining
      {...props}
      form="kc-register-form"
      action={kcContext.url.registrationAction}
      finish={msgStr("kdsMake")}
      detailsTitle={msgStr("kdsDetailsTitle")}
      {...(kcContext.passwordRequired
        ? { password: { policies: kcContext.passwordPolicies } }
        : {})}
      below={<QuietLink href={kcContext.url.loginUrl}>{msgStr("kdsHaveAccount")}</QuietLink>}
    />
  );
}

/** The first sign-in, or a change that the app asked for: the person says who they are. */
export function Welcome(props: Page<"login-update-profile.ftl">) {
  const { kcContext, i18n } = props;
  if (!plainProfile(kcContext)) {
    return (
      <Suspense>
        <Other {...props} />
      </Suspense>
    );
  }
  return (
    <Joining
      {...props}
      form="kc-update-profile-form"
      action={kcContext.url.loginAction}
      finish={i18n.msgStr("kdsSave")}
      detailsTitle={i18n.msgStr("kdsCheckDetailsTitle")}
    />
  );
}

/** The first sign-in with Google, GitHub or Apple: the person picks their username, and checks
    what that company said about them. */
export function ReviewProfile(props: Page<"idp-review-user-profile.ftl">) {
  const { kcContext, i18n } = props;
  if (!plainProfile(kcContext)) {
    return (
      <Suspense>
        <Other {...props} />
      </Suspense>
    );
  }
  return (
    <Joining
      {...props}
      form="kc-idp-review-profile-form"
      action={kcContext.url.loginAction}
      finish={i18n.msgStr("kdsContinue")}
      detailsTitle={i18n.msgStr("kdsCheckDetailsTitle")}
    />
  );
}

export function NewEmail(props: Page<"update-email.ftl">) {
  const { kcContext, i18n } = props;
  const { msgStr } = i18n;
  if (!plainProfile(kcContext)) {
    return (
      <Suspense>
        <Other {...props} />
      </Suspense>
    );
  }
  return (
    <Joining
      {...props}
      form="kc-update-email-form"
      action={kcContext.url.loginAction}
      finish={msgStr("kdsSave")}
      detailsTitle={msgStr("kdsNewEmailTitle")}
      extra={
        <SwitchRow
          id="logout-sessions"
          name="logout-sessions"
          value="on"
          label={msgStr("kdsSignOutOthers")}
          defaultChecked
        />
      }
    />
  );
}
