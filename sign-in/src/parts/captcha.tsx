import { type FormEvent, useEffect, useRef, useState } from "react";

import { cn } from "./cn";
import { rule } from "./rows";

/**
 * The check that a person, not a program, sends a form. Keycloak does the check; the page shows
 * the widget of the service that the realm uses. Keycloak's own step gives the values of Google
 * reCAPTCHA. A Cloudflare Turnstile step (github.com/panpaul/keycloak-turnstile has these names)
 * gives `captchaRequired`, `captchaSiteKey`, `captchaAction` and `captchaLanguage`.
 */
export interface PersonCheck {
  service: "recaptcha" | "turnstile";
  siteKey: string;
  action?: string | undefined;
  language?: string | undefined;
  /** The service shows a widget. With no widget, the button of the form asks the service. */
  widget: boolean;
}

interface Given {
  recaptchaRequired?: boolean;
  recaptchaVisible?: boolean;
  recaptchaSiteKey?: string;
  recaptchaAction?: string;
  captchaRequired?: boolean;
  captchaSiteKey?: string;
  captchaAction?: string;
  captchaLanguage?: string;
  scripts?: string[];
}

const SCRIPTS = {
  recaptcha: "https://www.google.com/recaptcha/api.js",
  turnstile: "https://challenges.cloudflare.com/turnstile/v0/api.js",
};
/** The names that the widgets call when the person passed, and when that is too long ago. */
const PASSED = "kodosiPersonPassed";
const LAPSED = "kodosiPersonLapsed";
/** After this time with no widget, the form goes to Keycloak and Keycloak says what is wrong. */
const PATIENCE_MS = 8000;

export function personCheckOf(kcContext: object): PersonCheck | null {
  const given = kcContext as Given;
  if (given.recaptchaRequired && given.recaptchaSiteKey)
    return {
      service: "recaptcha",
      siteKey: given.recaptchaSiteKey,
      action: given.recaptchaAction,
      widget: !!given.recaptchaVisible || given.recaptchaAction === undefined,
    };
  if (given.captchaRequired && given.captchaSiteKey)
    return {
      service: "turnstile",
      siteKey: given.captchaSiteKey,
      action: given.captchaAction,
      language: given.captchaLanguage,
      widget: true,
    };
  return null;
}

/**
 * The state of the check for one form. `passed` is true when the form can go: the person passed
 * the widget, or the form has no widget. A person who is faster than the check does not wait at a
 * dead button: `hold` keeps the send, `held` says so, and the form goes when the check passes.
 */
export function usePersonCheck(kcContext: object, form: string) {
  const check = personCheckOf(kcContext);
  const [passed, setPassed] = useState(!check?.widget);
  const [held, setHeld] = useState(false);
  const free = useRef(false);
  const service = check?.service;
  const widget = !!check?.widget;
  const given = (kcContext as Given).scripts;

  useEffect(() => {
    if (!held) return;
    // The form goes when the check passes. A check that never ends does not keep it: Keycloak
    // then says what is wrong.
    const timer = window.setTimeout(
      () => {
        free.current = true;
        setHeld(false);
        (document.getElementById(form) as HTMLFormElement | null)?.requestSubmit();
      },
      passed ? 0 : PATIENCE_MS,
    );
    return () => clearTimeout(timer);
  }, [held, passed, form]);

  /** For the send of the form: true when the form waits for the check. */
  const hold = (event: FormEvent) => {
    if (passed || free.current) {
      free.current = false;
      return false;
    }
    event.preventDefault();
    setHeld(true);
    return true;
  };

  useEffect(() => {
    if (!service) return;
    const names = window as unknown as Record<string, unknown>;
    names[PASSED] = () => setPassed(true);
    names[LAPSED] = () => setPassed(false);
    // reCAPTCHA with no widget asks when the person presses the button, and then sends the form.
    names.onSubmitRecaptcha = () =>
      (document.getElementById(form) as HTMLFormElement | null)?.requestSubmit();
    // Keycloak names the script of its own step, and the frame of the page loads it. A step that
    // names none gets the script of its service here. The widget is on the page at this moment.
    const marker = service === "recaptcha" ? "recaptcha" : "turnstile";
    let own: HTMLScriptElement | undefined;
    if (!given?.some((src) => src.includes(marker))) {
      own = document.createElement("script");
      own.src = SCRIPTS[service];
      own.async = true;
      document.head.appendChild(own);
    }
    // A widget that never comes (a browser that blocks it) does not hold the form for ever.
    const patience = widget
      ? window.setTimeout(() => {
          if (!names.grecaptcha && !names.turnstile) setPassed(true);
        }, PATIENCE_MS)
      : 0;
    return () => {
      clearTimeout(patience);
      own?.remove();
      delete names[PASSED];
      delete names[LAPSED];
      delete names.onSubmitRecaptcha;
    };
  }, [service, widget, form, given]);

  return { check, held, hold };
}

/** What the button of the form needs when the service has no widget. */
export function askOnPress(check: PersonCheck | null) {
  if (!check || check.widget || check.service !== "recaptcha") return {};
  return {
    "data-sitekey": check.siteKey,
    "data-callback": "onSubmitRecaptcha",
    "data-action": check.action,
  };
}

/**
 * The widget, as a row of the capsule. Turnstile shows itself only when it needs the person, so
 * the row has no height until then, and the capsule grows when it comes.
 */
export function PersonCheckRow({ check, label }: { check: PersonCheck | null; label: string }) {
  const box = useRef<HTMLDivElement>(null);
  const [shown, setShown] = useState(false);
  const [look] = useState(() => ({
    theme: matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light",
    small: matchMedia("(max-width: 380px)").matches,
  }));

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const watch = new ResizeObserver(() => setShown(el.offsetHeight > 8));
    watch.observe(el);
    return () => watch.disconnect();
  }, []);

  if (!check?.widget) return null;
  return (
    <fieldset
      aria-label={label}
      className={cn("min-w-0", shown && rule, shown && "flex justify-center px-3 py-3")}
    >
      <div
        ref={box}
        className={cn(
          "max-w-full overflow-hidden rounded-md",
          // Turnstile takes the width that it gets.
          check.service === "turnstile" && !look.small && "w-full",
        )}
      >
        {check.service === "recaptcha" ? (
          <div
            className="g-recaptcha"
            data-sitekey={check.siteKey}
            data-action={check.action}
            data-theme={look.theme}
            data-size={look.small ? "compact" : "normal"}
            data-callback={PASSED}
            data-expired-callback={LAPSED}
          />
        ) : (
          <div
            className="cf-turnstile"
            data-sitekey={check.siteKey}
            data-action={check.action}
            data-language={check.language}
            data-theme={look.theme}
            data-size={look.small ? "compact" : "flexible"}
            data-appearance="interaction-only"
            data-callback={PASSED}
            data-expired-callback={LAPSED}
            data-error-callback={LAPSED}
          />
        )}
      </div>
    </fieldset>
  );
}
