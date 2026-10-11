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
import { Shell } from "../frame/shell";
import { useCursor } from "../frame/use-cursor";
import { isInjection } from "../parts/captcha";
import { cn } from "../parts/cn";
import { Languages } from "../parts/languages";
import { IdentityRow, Note, quietLink } from "../parts/text";
import type { I18n } from "./i18n";
import type { KcContext } from "./KcContext";

/** The small sign of a state, before the title: the green check that draws itself, or the
    caution triangle. They are the signs of a terminal that is done or that failed in the app. */
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

/**
 * The frame of every page: the Kodosi mark, the step in one line, the capsule with the one thing
 * that the step needs, and the other ways under it. Keycloak's page loads are steps of one page
 * here: the mark stays in its place, the capsule and the cursor carry on from the page before,
 * and the title of that page leaves through the line that the new title comes through.
 */
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
  /** The name of the step in the browser's tab, when the title is not words alone. */
  tab?: string;
  title: ReactNode;
  lead?: ReactNode;
  /** The state of the step, as a small sign before the title. */
  sign?: "done" | "failed";
  /** The page is an end: the cursor stays still in the mark. */
  rest?: boolean;
  /** The page is the end of a sign-in: the mark is whole, as on the banner. */
  banner?: boolean;
  /** The account is gone: the cursor of the mark is an outline, as the cursor of a terminal that
      has nobody at it. */
  ended?: boolean;
  /** Keycloak refused what the person gave: the capsule shakes its head. */
  wrong?: boolean;
  /** The words for what was wrong, when the page has them for its own fields. */
  problem?: string | undefined;
  /** A page that checks a field itself counts its refusals: each one shows again. */
  attempt?: number;
  /** The page says Keycloak's message in its own place. */
  quiet?: boolean;
  /** The page holds the send for a check of its own: the capsule shows that it works. */
  waits?: boolean;
  /** More room, for a step with more than one thing in it. */
  wide?: boolean;
  /** Under the capsule: quiet links and other ways. */
  below?: ReactNode;
  /** At the foot of the page, in place of the languages. */
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

  // Keycloak's message for the whole page. A warning about an action that the app asked for
  // tells the person nothing new.
  const said =
    !quiet && message && (message.type !== "warning" || !isAppInitiatedAction)
      ? message
      : undefined;
  // A refusal shows until the person types an answer to it.
  const refusal =
    wrong || said?.type === "error" ? `${attempt}:${problem ?? said?.summary ?? ""}` : undefined;
  const refused = refusal !== undefined && answered !== refusal;
  const before = carried()?.title;
  const continues = carried() !== null;
  // The line of the title rolls when a step changes it on the same page, as between two pages.
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

  // A page with no capsule takes away what index.html drew of the page before. The end of a
  // sign-in takes it into the mark.
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
    // A page that comes back from the browser's history takes input again.
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
    // A request that never ends gives the capsule back.
    const timer = window.setTimeout(() => setBusy(false), 20_000);
    return () => clearTimeout(timer);
  }, [busy]);

  // The end of a sign-in, as the end of the launch film: what the page before held goes into the
  // cursor, and the mark of the head grows into the whole mark of the banner.
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
      // The scripts that a step of Keycloak asks for: the check that a person sends the form.
      // A check that puts its own widget on the page is drawn by the page itself (parts/captcha).
      ...(kcContext.scripts ?? [])
        .filter((src) => !isInjection(src))
        .map((src) => ({ type: "text/javascript" as const, src })),
      // Keycloak's own watch: a sign-in in a different tab carries this tab on.
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
    <div className="flex min-h-dvh flex-col" onInput={typed} onSubmit={submit}>
      <main
        className={cn(
          "mx-auto flex w-full flex-col items-center px-4 pt-(--stage-top) pb-10",
          wide ? "max-w-[520px]" : "max-w-[432px]",
          !continues && "rise",
        )}
      >
        {/* The room of the mark, which index.html draws. */}
        <div aria-hidden className={banner ? "h-[150px]" : "h-[calc(var(--logo-size)*1.5)]"} />
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
            <Shell
              ref={shell}
              busy={busy}
              waits={waits}
              wrong={refused ? refusal : undefined}
              grows
            >
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
              tone={
                said.type === "error" ? "danger" : said.type === "warning" ? "caution" : "plain"
              }
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
      </main>
      <footer className="mt-auto flex justify-center px-4 pb-[max(1.5rem,env(safe-area-inset-bottom))]">
        {foot ?? (
          <Languages
            label={msgStr("kdsLanguage")}
            current={i18n.currentLanguage.languageTag}
            languages={i18n.enabledLanguages.map(({ languageTag, label, href }) => ({
              tag: languageTag,
              label,
              href,
            }))}
          />
        )}
      </footer>
    </div>
  );
}
