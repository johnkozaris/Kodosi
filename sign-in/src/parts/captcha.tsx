import { RotateCw, TriangleAlert } from "lucide-react";
import { type FormEvent, useCallback, useEffect, useRef, useState } from "react";

import { cn } from "./cn";
import { RowAction, rule } from "./rows";

/**
 * The check that a person, not a program, sends a form. It is on the form that makes an account
 * and on the form that sends a link for a new password, never on the sign-in: a person with an
 * account always reaches the sign-in. Keycloak does the check, and the page shows the widget of
 * the service that the realm uses. A realm with no check shows nothing.
 *
 * Each Keycloak step for a check gives the page its own names:
 * - Keycloak's reCAPTCHA step: recaptchaRequired, recaptchaSiteKey, recaptchaAction, recaptchaVisible.
 * - The Turnstile step of github.com/zymlabs/keycloak-cloudflare-turnstile-provider:
 *   turnstileRequired, turnstileSiteKey, turnstileMode. On the reset form it gives only the two
 *   scripts that put its widget on a page, and the address of one of them has the site key.
 * - The Turnstile step of github.com/panpaul/keycloak-turnstile: captchaRequired, captchaSiteKey,
 *   captchaAction, captchaLanguage.
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
  turnstileRequired?: boolean;
  turnstileSkipped?: boolean;
  turnstileSiteKey?: string;
  turnstileMode?: string;
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
const OWN_SCRIPT = {
  recaptcha: /\/\/(www\.)?(google\.com|recaptcha\.net)\/recaptcha\//,
  turnstile: /\/\/challenges\.cloudflare\.com\/turnstile\//,
};
/** The names that the widgets call when the person passed, failed, or passed too long ago. */
const PASSED = "kodosiPersonPassed";
const FAILED = "kodosiPersonFailed";
const LAPSED = "kodosiPersonLapsed";
/** After this time with no widget, the form goes to Keycloak and Keycloak says what is wrong. */
const PATIENCE_MS = 8000;

/** The keys of Keycloak's messages that say the check refused the form. */
export const REFUSALS = [
  "recaptchaFailed",
  "recaptchaNotConfigured",
  "turnstileVerificationFailed",
  "turnstileMissingToken",
  "turnstileVerificationError",
  "turnstileIpBlocked",
] as const;

/** A script with which a Turnstile step puts its own widget on a page. The page draws the
    widget itself, so it does not load these. */
export function isInjection(src: string): boolean {
  return /\/turnstile-injector\.js([?#]|$)/.test(src) || /\/config\.js\?(.*&)?siteKey=/.test(src);
}

/** The site key in the address of an injection script. */
function injectedKey(scripts: string[] | undefined): string | null {
  for (const src of scripts ?? []) {
    if (!isInjection(src)) continue;
    try {
      const key = new URL(src, location.href).searchParams.get("siteKey");
      if (key) return key;
    } catch {
      // An address that does not parse names no key.
    }
  }
  return null;
}

export function personCheckOf(kcContext: object): PersonCheck | null {
  const given = kcContext as Given;
  if (given.recaptchaRequired && given.recaptchaSiteKey)
    return {
      service: "recaptcha",
      siteKey: given.recaptchaSiteKey,
      action: given.recaptchaAction,
      widget: !!given.recaptchaVisible || given.recaptchaAction === undefined,
    };
  if (given.turnstileRequired && !given.turnstileSkipped && given.turnstileSiteKey)
    return { service: "turnstile", siteKey: given.turnstileSiteKey, widget: true };
  if (given.captchaRequired && given.captchaSiteKey)
    return {
      service: "turnstile",
      siteKey: given.captchaSiteKey,
      action: given.captchaAction,
      language: given.captchaLanguage,
      widget: true,
    };
  const injected = injectedKey(given.scripts);
  if (injected) return { service: "turnstile", siteKey: injected, widget: true };
  return null;
}

/** The widget of the service, once it is loaded. */
interface Widgets {
  turnstile?: { reset(target?: string | HTMLElement): void };
  grecaptcha?: { reset(id?: number): void };
}

/**
 * The state of the check for one form. `passed` is true when the form can go: the person passed
 * the widget, or the form has no widget. A person who is faster than the check does not wait at a
 * dead button: `hold` keeps the send, `held` says so, and the form goes when the check passes.
 * `failed` is true when the widget said no: the row shows it, and `again` starts a new check.
 */
export function usePersonCheck(kcContext: object, form: string) {
  const check = personCheckOf(kcContext);
  const [passed, setPassed] = useState(!check?.widget);
  const [failed, setFailed] = useState(false);
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

  const again = useCallback(() => {
    const names = window as unknown as Widgets;
    setFailed(false);
    setPassed(false);
    if (service === "turnstile") names.turnstile?.reset();
    else names.grecaptcha?.reset();
  }, [service]);

  useEffect(() => {
    if (!service) return;
    const names = window as unknown as Record<string, unknown>;
    names[PASSED] = () => {
      setFailed(false);
      setPassed(true);
    };
    // A check that says no gives the form back: the row says why.
    names[FAILED] = () => {
      setPassed(false);
      setFailed(true);
      setHeld(false);
    };
    names[LAPSED] = () => setPassed(false);
    // reCAPTCHA with no widget asks when the person presses the button, and then sends the form.
    names.onSubmitRecaptcha = () =>
      (document.getElementById(form) as HTMLFormElement | null)?.requestSubmit();
    // Keycloak can name the script of the service, and the frame of the page loads it then. A
    // step that names none gets the script here. The widget is on the page at this moment.
    let own: HTMLScriptElement | undefined;
    if (!given?.some((src) => OWN_SCRIPT[service].test(src))) {
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
      delete names[FAILED];
      delete names[LAPSED];
      delete names.onSubmitRecaptcha;
    };
  }, [service, widget, form, given]);

  return { check, passed, held, hold, failed, again };
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
 * the row has no height until then, and the capsule grows when it comes. When the check says no,
 * here or at Keycloak, the row says so in one line and offers a new check.
 */
export function PersonCheckRow({
  check,
  label,
  problem,
  againLabel,
  onAgain,
}: {
  check: PersonCheck | null;
  label: string;
  /** The words of a check that said no. */
  problem?: string | undefined;
  againLabel: string;
  onAgain: () => void;
}) {
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
    <fieldset aria-label={label} className={cn("min-w-0", (shown || problem) && rule)}>
      <div className={cn(shown && "flex justify-center px-3 py-3")}>
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
              data-error-callback={FAILED}
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
              data-error-callback={FAILED}
            />
          )}
        </div>
      </div>
      {problem && (
        <div
          role="alert"
          className="resolve flex min-h-12 items-center gap-2.5 py-1.5 pr-2.5 pl-[18px]"
        >
          <TriangleAlert className="size-4 shrink-0 text-caution" strokeWidth={2.2} aria-hidden />
          <span className="min-w-0 flex-1 text-footnote text-ink">{problem}</span>
          <RowAction onClick={onAgain}>
            <span className="inline-flex items-center gap-1.5">
              <RotateCw className="size-3.5" strokeWidth={2.4} aria-hidden />
              {againLabel}
            </span>
          </RowAction>
        </div>
      )}
    </fieldset>
  );
}
