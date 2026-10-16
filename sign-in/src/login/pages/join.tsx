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
import { cn } from "../../parts/cn";
import { Avatar } from "../../parts/person";
import { Go, PasswordRow, rule, SignOutOthers, SwitchRow, TextRow } from "../../parts/rows";
import { plain, QuietLink, quietLink, safeHtml } from "../../parts/text";
import { askOnPress, PersonCheckRow, usePersonCheck } from "../captcha";
import type { I18n } from "../i18n";
import { Stage } from "../stage";
import { meetsPolicy, PolicyTags } from "./policy";
import type { Page } from "./props";

const Other = lazy(() => import("./other"));

type Joins = Page<
  "register.ftl" | "login-update-profile.ftl" | "idp-review-user-profile.ftl" | "update-email.ftl"
>;

const PLAIN = new Set([undefined, "text", "html5-email", "html5-tel", "html5-url"]);

const SHORT_TERMS = 120;

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

function asHandle(typed: string): string {
  return typed.replace(/^@+/, "").replace(/\s+/g, "").toLowerCase();
}

function patternWords(i18n: I18n, attribute: Attribute): string {
  const words = attribute.validators.pattern?.["error-message"];
  return words ? i18n.advancedMsgStr(words) : i18n.msgStr("kdsBadCharacters");
}

function breaksPattern(attribute: Attribute, typed: string): boolean {
  const pattern = attribute.validators.pattern?.pattern;
  if (!pattern || !typed) return false;
  try {
    return !new RegExp(pattern).test(typed);
  } catch {
    return false;
  }
}

function problemOf(i18n: I18n, attribute: Attribute, value: string): string | null {
  const { msgStr } = i18n;
  const typed = value.trim();
  if (!typed) return attribute.required ? msgStr("kdsFill") : null;
  const { length, email } = attribute.validators;
  if ((email || attribute.name === "email") && !/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(typed))
    return msgStr("kdsBadEmail");
  if (breaksPattern(attribute, typed)) return patternWords(i18n, attribute);
  const min = Number(length?.min ?? 0);
  const max = Number(length?.max ?? 0);
  const letters = [...typed].length;
  if (letters < min || (max > 0 && letters > max))
    return max > 0 ? msgStr("kdsBadLength", `${min}`, `${max}`) : msgStr("kdsTooShort", `${min}`);
  return null;
}

type StepId = "handle" | "details" | "password";

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

function Optional({ label }: { label: string }) {
  return <span className="pr-2 text-caption text-ink-faint">{label}</span>;
}

type JoiningProps = Joins & {
  form: string;
  action: string;
  finish: string;
  detailsTitle: string;
  password?: { policies: PasswordPolicies | undefined };
  extra?: ReactNode;
  below?: ReactNode;
};

function Joining(props: JoiningProps) {
  if (plainProfile(props.kcContext)) return <Steps {...props} />;
  return (
    <Suspense>
      <Other kcContext={props.kcContext} i18n={props.i18n} />
    </Suspense>
  );
}

function Steps({
  kcContext,
  i18n,
  form,
  action,
  finish,
  detailsTitle,
  password,
  extra,
  below,
}: JoiningProps) {
  const { profile, messagesPerField, isAppInitiatedAction, message } = kcContext;
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
  const handleField = useRef<HTMLInputElement>(null);
  const check = usePersonCheck(kcContext, form, i18n);

  const refusedAt = attributes.find((one) => messagesPerField.existsError(one.name));
  const secretRefused = messagesPerField.existsError("password", "password-confirm");
  const termsRefused = messagesPerField.existsError("termsAccepted");
  const [step, setStep] = useState<StepId>(() => {
    if (refusedAt) return refusedAt === handleAttribute ? "handle" : "details";
    if (secretRefused || termsRefused || message?.type === "error") return last as StepId;
    return steps[0] as StepId;
  });
  const [found, setFound] = useState<{ field: string; text: string; attempt: number } | null>(null);
  const moved = useRef(false);
  const body = useRef<HTMLFormElement>(null);

  useEffect(() => {
    if (!moved.current) return;
    const first = body.current?.querySelector<HTMLInputElement>(
      `[data-step="${step}"] input:not([type=hidden],[type=checkbox])`,
    );
    first?.focus();
  }, [step]);

  const handle = values.username ?? "";
  const handleBreaks = !!handleAttribute && breaksPattern(handleAttribute, handle);
  useEffect(() => {
    handleField.current?.setCustomValidity(
      handleBreaks && handleAttribute ? patternWords(i18n, handleAttribute) : "",
    );
  }, [handleBreaks, handleAttribute, i18n]);

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
  const sound = (): boolean => {
    const fields =
      step === "handle" ? [handleAttribute as Attribute] : step === "details" ? details : [];
    for (const attribute of fields) {
      const text = problemOf(i18n, attribute, values[attribute.name] ?? "");
      if (text) return refuse(attribute.name, text);
    }
    if (step === "password" && !secret) return refuse("password", msgStr("kdsFill"));
    if (step === last && terms && !accepted)
      return refuse("termsAccepted", msgStr("termsAcceptanceRequired"));
    return true;
  };
  const next = () => {
    if (sound()) go(steps[steps.indexOf(step) + 1] as StepId);
  };
  const enter = (event: KeyboardEvent) => {
    if (event.key !== "Enter" || step === last || event.nativeEvent.isComposing) return;
    event.preventDefault();
    next();
  };
  const change = (name: string, value: string) => setValues((now) => ({ ...now, [name]: value }));

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
  const termsText = terms ? safeHtml(msgStr("termsText")) : "";
  const termsShort = !!termsText && plain(termsText).length <= SHORT_TERMS;

  const finishing = (
    <>
      {terms && (
        <>
          <SwitchRow
            id="termsAccepted"
            name="termsAccepted"
            label={
              <>
                {msgStr("kdsAcceptTerms")}
                {termsShort && (
                  <span
                    className="prose-terms mt-0.5 block text-footnote text-ink-muted"
                    dangerouslySetInnerHTML={{ __html: termsText }}
                  />
                )}
                {termsText && !termsShort && (
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
            aria-invalid={termsRefused || wrongField === "termsAccepted" || undefined}
          />
          {reads && !termsShort && (
            <div
              id="kc-registration-terms-text"
              className={cn(
                rule,
                "prose-terms resolve max-h-[36vh] overflow-y-auto px-[18px] py-4 text-[14px]",
              )}
              dangerouslySetInnerHTML={{ __html: termsText }}
            />
          )}
        </>
      )}
      <PersonCheckRow check={check} i18n={i18n} />
      <div className={cn(rule, "p-3")}>
        <button
          type="submit"
          {...askOnPress(check.config)}
          className={buttonClass({
            variant: "primary",
            size: "lg",
            className: cn("w-full", check.config && !check.config.widget && "g-recaptcha"),
          })}
        >
          {finish}
        </button>
      </div>
    </>
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
      wrong={!!found || !!serverProblem || (check.refused && !check.retried)}
      problem={found?.text ?? serverProblem}
      attempt={found?.attempt ?? 0}
      quiet={!!serverProblem || check.refused || message?.type === "warning"}
      waits={check.held}
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
          if (step !== last || !sound()) {
            event.preventDefault();
            if (step !== last) next();
            return;
          }
          if (check.hold(event)) return;
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
                  ref={handleField}
                  id="username"
                  name="username"
                  className="handle-input"
                  aria-label={msgStr("kdsUsername")}
                  aria-invalid={wrongField === "username" || handleBreaks || undefined}
                  aria-describedby={handleBreaks ? "handle-rule" : undefined}
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
                  minLength={Number(handleAttribute.validators.length?.min ?? 0) || undefined}
                  maxLength={Number(handleAttribute.validators.length?.max ?? 0) || undefined}
                />
              </label>
              {last === "handle" ? null : (
                <Go type="button" label={msgStr("kdsNext")} onClick={next} />
              )}
            </div>
            <div aria-live="polite">
              {handleBreaks && (
                <p
                  id="handle-rule"
                  className={cn(rule, "resolve px-[18px] py-2.5 text-footnote text-caution")}
                >
                  {patternWords(i18n, handleAttribute)}
                </p>
              )}
            </div>
            {last === "handle" && finishing}
          </div>
        )}
        {details.length > 0 && (
          <div data-step="details" className={step === "details" ? "resolve" : "hidden"}>
            {details.map((attribute, i) => {
              const value = values[attribute.name] ?? "";
              const arrow = last !== "details" && i === details.length - 1;
              const optional = !attribute.required && !attribute.readOnly && !value;
              return (
                <TextRow
                  key={attribute.name}
                  id={attribute.name}
                  name={attribute.name}
                  label={labelOf(i18n, attribute)}
                  type={attribute.annotations.inputType?.replace("html5-", "") ?? "text"}
                  value={value}
                  onChange={(event) => change(attribute.name, event.target.value)}
                  onKeyDown={enter}
                  readOnly={attribute.readOnly}
                  required={attribute.required}
                  autoComplete={attribute.autocomplete}
                  autoFocus={step === "details" && i === 0}
                  aria-invalid={wrongField === attribute.name || undefined}
                  end={
                    optional || arrow ? (
                      <>
                        {optional && <Optional label={msgStr("kdsOptional")} />}
                        {arrow && <Go type="button" label={msgStr("kdsNext")} onClick={next} />}
                      </>
                    ) : undefined
                  }
                />
              );
            })}
            {extra}
            {last === "details" && finishing}
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
              fits={meetsPolicy(i18n, password.policies, secret, {
                username: handle,
                email: values.email,
              })}
              show={msgStr("kdsShowPassword")}
              hide={msgStr("kdsHidePassword")}
              capsLock={msgStr("kdsCapsLock")}
            />
            {/* Keycloak asks for the password two times: the form sends it again here. */}
            <input ref={again} type="hidden" id="password-confirm" name="password-confirm" />
            <PolicyTags
              i18n={i18n}
              policies={password.policies}
              password={secret}
              username={handle}
              email={values.email}
            />
            {finishing}
          </div>
        )}
      </form>
    </Stage>
  );
}

export function Register(props: Page<"register.ftl">) {
  const { kcContext, i18n } = props;
  const { msgStr } = i18n;
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

export function Welcome(props: Page<"login-update-profile.ftl">) {
  const { kcContext, i18n } = props;
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

export function ReviewProfile(props: Page<"idp-review-user-profile.ftl">) {
  const { kcContext, i18n } = props;
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
  return (
    <Joining
      {...props}
      form="kc-update-email-form"
      action={kcContext.url.loginAction}
      finish={msgStr("kdsSave")}
      detailsTitle={msgStr("kdsNewEmailTitle")}
      extra={<SignOutOthers shown label={msgStr("kdsSignOutOthers")} />}
    />
  );
}
