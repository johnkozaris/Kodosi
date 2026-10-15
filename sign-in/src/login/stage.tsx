import { useInsertScriptTags } from "keycloakify/tools/useInsertScriptTags";
import { TriangleAlert } from "lucide-react";
import {
  type FormEvent,
  type ReactNode,
  type SyntheticEvent,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";

import { carried, dropDrawn, handOver } from "../frame/carry";
import { play, springSoft, still } from "../frame/motion";
import { Page } from "../frame/page";
import { Shell } from "../frame/shell";
import { useCursor } from "../frame/use-cursor";
import { cn } from "../parts/cn";
import { Languages } from "../parts/languages";
import { Legal } from "../parts/legal";
import { IdentityRow, Note, quietLink } from "../parts/text";
import { isInjection } from "./captcha";
import type { I18n } from "./i18n";
import type { KcContext } from "./KcContext";

function Sign({ form }: { form: "done" | "failed" }) {
  if (form === "failed")
    return (
      <TriangleAlert
        className="pop mr-2.5 inline-block size-[21px] align-[-3px] text-caution"
        strokeWidth={2.2}
        aria-hidden
      />
    );
  return (
    <svg viewBox="0 0 24 24" className="mr-2 inline-block size-6 align-[-4px]" aria-hidden>
      <path className="check-draw" pathLength={1} strokeWidth={3} d="M4.6 12.8l5 4.8 9.8-10.8" />
    </svg>
  );
}

export function Stage({
  kcContext,
  i18n,
  tab,
  title,
  lead,
  sign,
  rest,
  banner,
  ended,
  wrong,
  problem,
  attempt = 0,
  quiet,
  waits,
  wide,
  below,
  foot,
  children,
}: {
  kcContext: KcContext;
  i18n: I18n;
  tab?: string;
  title: ReactNode;
  lead?: ReactNode;
  sign?: "done" | "failed";
  rest?: boolean;
  banner?: boolean;
  ended?: boolean;
  wrong?: boolean;
  problem?: string | undefined;
  attempt?: number;
  quiet?: boolean;
  waits?: boolean;
  wide?: boolean;
  below?: ReactNode;
  foot?: ReactNode;
  children?: ReactNode;
}) {
  const { url, message, auth, isAppInitiatedAction } = kcContext;
  const { msgStr } = i18n;
  const shell = useRef<HTMLDivElement>(null);
  const [busy, setBusy] = useState(false);
  const [answered, setAnswered] = useState<string | null>(null);
  const cursor = useCursor({ rest, busy: busy || !!waits });
  const name = tab ?? (typeof title === "string" ? title : msgStr("kdsSignIn"));

  const said =
    !quiet && message && (message.type !== "warning" || !isAppInitiatedAction)
      ? message
      : undefined;
  const refusal =
    wrong || said?.type === "error" ? `${attempt}:${problem ?? said?.summary ?? ""}` : undefined;
  const refused = refusal !== undefined && answered !== refusal;
  const before = carried()?.title;
  const continues = carried() !== null;
  const [roll, setRoll] = useState<{ out: string | undefined; turn: number }>(() => ({
    out: before && before !== name ? before : undefined,
    turn: 0,
  }));
  const shownName = useRef(name);
  useLayoutEffect(() => {
    if (shownName.current === name) return;
    const out = shownName.current;
    shownName.current = name;
    setRoll((now) => ({ out, turn: now.turn + 1 }));
  }, [name]);

  useEffect(() => {
    document.title = `${name} · Kodosi`;
  }, [name]);

  // oxlint-disable-next-line react-hooks/exhaustive-deps -- what index.html drew is there one time
  useLayoutEffect(() => (banner ? undefined : dropDrawn()), []);

  useLayoutEffect(() => {
    if (!ended) return;
    document.documentElement.dataset.ended = "";
    return () => {
      delete document.documentElement.dataset.ended;
    };
  }, [ended]);

  useLayoutEffect(() => {
    const leave = () => handOver(shell.current, cursor.current?.box() ?? null, name);
    const back = (event: PageTransitionEvent) => {
      if (event.persisted) setBusy(false);
    };
    window.addEventListener("pagehide", leave);
    window.addEventListener("pageshow", back);
    return () => {
      window.removeEventListener("pagehide", leave);
      window.removeEventListener("pageshow", back);
    };
  }, [cursor, name]);

  useEffect(() => {
    if (!busy) return;
    const timer = window.setTimeout(() => setBusy(false), 20_000);
    return () => clearTimeout(timer);
  }, [busy]);

  useLayoutEffect(() => {
    const logo = document.getElementById("logo");
    if (!logo || !banner) return;
    const parts = [...logo.children];
    const from = parts.map((part) => part.getBoundingClientRect());
    logo.dataset.banner = "";
    const drawn = document.getElementById("carry");
    const home = document.getElementById("cursor-home")?.getBoundingClientRect();
    let into: Animation | undefined;
    if (drawn && home && continues && !still()) {
      const box = drawn.getBoundingClientRect();
      drawn.style.transformOrigin = "0 0";
      into = play(
        drawn,
        [
          { transform: "none", opacity: 1 },
          {
            transform: `translate(${home.left - box.left}px, ${home.top - box.top}px) scale(${home.width / box.width}, ${home.height / box.height})`,
            backgroundColor: "var(--accent)",
            opacity: 1,
            offset: 0.85,
          },
          {
            transform: `translate(${home.left - box.left}px, ${home.top - box.top}px) scale(${home.width / box.width}, ${home.height / box.height})`,
            backgroundColor: "var(--accent)",
            opacity: 0,
          },
        ],
        springSoft,
        { fill: "forwards" },
      );
      into.finished.then(
        () => drawn.remove(),
        () => drawn.remove(),
      );
    } else dropDrawn();
    const moves = continues
      ? parts.map((part, i) => {
          const to = part.getBoundingClientRect();
          const was = from[i] as DOMRect;
          return play(
            part,
            [
              {
                transformOrigin: "0 0",
                transform: `translate(${was.left - to.left}px, ${was.top - to.top}px) scale(${was.width / to.width})`,
              },
              { transformOrigin: "0 0", transform: "none" },
            ],
            springSoft,
          );
        })
      : [];
    return () => {
      for (const move of moves) move.cancel();
      into?.cancel();
      delete logo.dataset.banner;
    };
  }, [banner, continues]);

  const { insertScriptTags } = useInsertScriptTags({
    componentOrHookName: "Stage",
    scriptTags: [
      ...(kcContext.scripts ?? [])
        .filter((src) => !isInjection(src))
        .map((src) => ({ type: "text/javascript" as const, src })),
      {
        type: "module",
        textContent: [
          `import { startSessionPolling, checkAuthSession } from "${url.resourcesPath}/js/authChecker.js";`,
          `startSessionPolling("${url.ssoLoginInOtherTabsUrl}");`,
          kcContext.authenticationSession
            ? `checkAuthSession("${kcContext.authenticationSession.authSessionIdHash}");`
            : "",
        ].join("\n"),
      },
    ],
  });
  useEffect(() => {
    insertScriptTags();
  }, [insertScriptTags]);

  const typed = (event: SyntheticEvent) => {
    if (event.target instanceof HTMLInputElement && refusal) setAnswered(refusal);
  };
  const submit = (event: FormEvent) => {
    if (busy) event.preventDefault();
    else if (!event.defaultPrevented) setBusy(true);
  };

  return (
    <Page
      wide={wide}
      banner={banner}
      onInput={typed}
      onSubmit={submit}
      foot={
        foot ?? (
          <>
            <Languages
              label={msgStr("kdsLanguage")}
              current={i18n.currentLanguage.languageTag}
              languages={i18n.enabledLanguages.map(({ languageTag, label, href }) => ({
                tag: languageTag,
                label,
                href,
              }))}
            />
            <Legal
              links={[
                { label: msgStr("kdsTerms"), href: kcContext.properties.KODOSI_TERMS_URL },
                { label: msgStr("kdsPrivacy"), href: kcContext.properties.KODOSI_PRIVACY_URL },
              ]}
            />
          </>
        )
      }
    >
      <div className={cn("w-full text-center", banner ? "mt-9" : "mt-8")}>
        <h1 className="roll font-mono text-title font-medium text-ink">
          {roll.out && (
            <span key={`out-${roll.turn}`} aria-hidden className="roll-out">
              {roll.out}
            </span>
          )}
          <span
            key={`in-${roll.turn}`}
            className={roll.turn > 0 || (before && before !== name) ? "roll-in" : undefined}
          >
            {sign && <Sign form={sign} />}
            {title}
          </span>
        </h1>
        {lead && (
          <div
            key={`lead-${roll.turn}`}
            className={cn(
              "mx-auto mt-2.5 max-w-[40ch] text-callout text-balance text-ink-muted",
              (continues || roll.turn > 0) && "resolve",
            )}
          >
            {lead}
          </div>
        )}
      </div>
      {children && (
        <div className="mt-7 w-full">
          <Shell ref={shell} busy={busy} waits={waits} wrong={refused ? refusal : undefined}>
            {auth?.showUsername && !auth.showResetCredentials && auth.attemptedUsername && (
              <IdentityRow
                name={auth.attemptedUsername}
                restart={url.loginRestartFlowUrl}
                restartLabel={msgStr("kdsNotYou")}
              />
            )}
            {children}
          </Shell>
        </div>
      )}
      <div aria-live="polite" className="w-full">
        {refused && problem && <Note tone="danger" text={problem} />}
        {said && !(refused && problem) && (
          <Note
            tone={said.type === "error" ? "danger" : said.type === "warning" ? "caution" : "plain"}
            text={said.summary}
          />
        )}
      </div>
      {(below || auth?.showTryAnotherWayLink) && (
        <div
          className={cn("mt-5 flex w-full flex-col items-center gap-2.5", continues && "resolve")}
        >
          {below}
          {auth?.showTryAnotherWayLink && (
            <form action={url.loginAction} method="post">
              <input type="hidden" name="tryAnotherWay" value="on" />
              <button type="submit" className={quietLink}>
                {msgStr("kdsAnotherWay")}
              </button>
            </form>
          )}
        </div>
      )}
    </Page>
  );
}
