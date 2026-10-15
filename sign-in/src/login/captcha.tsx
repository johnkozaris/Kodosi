import { RotateCw, TriangleAlert } from "lucide-react";
import { type FormEvent, useCallback, useEffect, useRef, useState } from "react";

import { cn } from "../parts/cn";
import { RowAction, rule } from "../parts/rows";
import { type I18n, isMessage } from "./i18n";
import type { KcContext } from "./KcContext";

interface PersonCheck {
  service: "recaptcha" | "turnstile";
  siteKey: string;
  action?: string | undefined;
  language?: string | undefined;
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
const PASSED = "kodosiPersonPassed";
const FAILED = "kodosiPersonFailed";
const LAPSED = "kodosiPersonLapsed";
const PATIENCE_MS = 8000;

const REFUSALS = [
  "recaptchaFailed",
  "recaptchaNotConfigured",
  "turnstileVerificationFailed",
  "turnstileMissingToken",
  "turnstileVerificationError",
  "turnstileIpBlocked",
] as const;

export function isInjection(src: string): boolean {
  return /\/turnstile-injector\.js([?#]|$)/.test(src) || /\/config\.js\?(.*&)?siteKey=/.test(src);
}

function injectedKey(scripts: string[] | undefined): string | null {
  for (const src of scripts ?? []) {
    if (!isInjection(src)) continue;
    try {
      const key = new URL(src, location.href).searchParams.get("siteKey");
      if (key) return key;
    } catch {}
  }
  return null;
}

// recaptcha*: Keycloak's reCAPTCHA step. captcha*: github.com/panpaul/keycloak-turnstile.
// turnstile* and the injected scripts of the reset form:
// github.com/zymlabs/keycloak-cloudflare-turnstile-provider.
function personCheckOf(kcContext: object): PersonCheck | null {
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

interface Widgets {
  turnstile?: { reset(target?: string | HTMLElement): void };
  grecaptcha?: { reset(id?: number): void };
}

export function usePersonCheck(kcContext: KcContext, form: string, i18n: I18n) {
  const config = personCheckOf(kcContext);
  const [passed, setPassed] = useState(!config?.widget);
  const [failed, setFailed] = useState(false);
  const [held, setHeld] = useState(false);
  const [retried, setRetried] = useState(false);
  const free = useRef(false);
  const service = config?.service;
  const widget = !!config?.widget;
  const given = (kcContext as Given).scripts;

  useEffect(() => {
    if (!held) return;
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

  const again = useCallback(() => {
    const names = window as unknown as Widgets;
    setFailed(false);
    setPassed(false);
    if (service === "turnstile") names.turnstile?.reset();
    else names.grecaptcha?.reset();
  }, [service]);

  const hold = (event: FormEvent) => {
    if (passed || free.current) {
      free.current = false;
      return false;
    }
    event.preventDefault();
    if (failed) again();
    setHeld(true);
    return true;
  };

  useEffect(() => {
    if (!service) return;
    const names = window as unknown as Record<string, unknown>;
    names[PASSED] = () => {
      setFailed(false);
      setPassed(true);
    };
    names[FAILED] = () => {
      setPassed(false);
      setFailed(true);
      setHeld(false);
    };
    names[LAPSED] = () => setPassed(false);
    names.onSubmitRecaptcha = () =>
      (document.getElementById(form) as HTMLFormElement | null)?.requestSubmit();
    let own: HTMLScriptElement | undefined;
    if (!given?.some((src) => OWN_SCRIPT[service].test(src))) {
      own = document.createElement("script");
      own.src = SCRIPTS[service];
      own.async = true;
      document.head.appendChild(own);
    }
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

  const { message } = kcContext;
  const refused =
    message?.type === "error" && REFUSALS.some((key) => isMessage(i18n, message.summary, key));
  const problem = failed
    ? i18n.msgStr("kdsCheckFailed")
    : refused && !retried
      ? message.summary
      : undefined;
  const retry = () => {
    setRetried(true);
    again();
  };

  return { config, passed, held, hold, failed, refused, retried, problem, retry };
}

export function askOnPress(config: PersonCheck | null) {
  if (!config || config.widget || config.service !== "recaptcha") return {};
  return {
    "data-sitekey": config.siteKey,
    "data-callback": "onSubmitRecaptcha",
    "data-action": config.action,
  };
}

export function PersonCheckRow({
  check,
  i18n,
}: {
  check: ReturnType<typeof usePersonCheck>;
  i18n: I18n;
}) {
  const { config, problem } = check;
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

  if (!config?.widget) return null;
  return (
    <fieldset
      aria-label={i18n.msgStr("kdsNotRobot")}
      className={cn("min-w-0", (shown || problem) && rule)}
    >
      <div className={cn(shown && "flex justify-center px-3 py-3")}>
        <div
          ref={box}
          className={cn(
            "max-w-full overflow-hidden rounded-md",
            config.service === "turnstile" && !look.small && "w-full",
          )}
        >
          {config.service === "recaptcha" ? (
            <div
              className="g-recaptcha"
              data-sitekey={config.siteKey}
              data-action={config.action}
              data-theme={look.theme}
              data-size={look.small ? "compact" : "normal"}
              data-callback={PASSED}
              data-expired-callback={LAPSED}
              data-error-callback={FAILED}
            />
          ) : (
            <div
              className="cf-turnstile"
              data-sitekey={config.siteKey}
              data-action={config.action}
              data-language={config.language}
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
          <RowAction onClick={check.retry}>
            <span className="inline-flex items-center gap-1.5">
              <RotateCw className="size-3.5" strokeWidth={2.4} aria-hidden />
              {i18n.msgStr("kdsTryAgain")}
            </span>
          </RowAction>
        </div>
      )}
    </fieldset>
  );
}
