import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { carried, dropDrawn, handOver } from "../frame/carry";
import { useCursor } from "../frame/use-cursor";
import { cn } from "../parts/cn";
import { Languages } from "../parts/languages";
import { Avatar } from "../parts/person";
import { Way as Button, Note, QuietLink, quietLink } from "../parts/text";
import { Apps, Details, LinkedAccounts, SignedIn, SignIn } from "./groups";
import type { KcContext } from "./KcContext";
import {
  type AccountService,
  type Linked,
  type Person,
  type Program,
  Refused,
  type Session,
  type Way,
} from "./service";
import { wordsOf } from "./words";

interface Facts {
  person: Person;
  ways: Way[];
  sessions: Session[];
  linked: Linked[];
  programs: Program[];
}

/** Null while Keycloak has not answered. */
type State = { service: AccountService; facts: Facts } | "unreachable" | null;

/** Asks Keycloak for each fact of the page, and tells what came. */
function ask(open: () => Promise<AccountService>, tell: (state: State) => void) {
  void gather(open).then(tell, () => tell("unreachable"));
}

async function gather(open: () => Promise<AccountService>) {
  const service = await open();
  const [person, ways, sessions, linked, programs] = await Promise.all([
    service.person(),
    service.ways(),
    service.sessions(),
    service.linked(),
    service.programs(),
  ]);
  return { service, facts: { person, ways, sessions, linked, programs } };
}

function detail(person: Person, name: string): string {
  return person.fields.find((field) => field.name === name)?.value.trim() ?? "";
}

/**
 * The account of a person: how they sign in, where they are signed in, and their details. It is
 * one page in place of Keycloak's account console. It has the frame of the sign-in pages, because
 * each change here is a step of those pages: the mark keeps its place, and the first capsule
 * carries on from the step and back to it. The head is the person as their friends see them:
 * their circle, their name and their username.
 */
export function Account({
  kcContext,
  open,
}: {
  kcContext: KcContext;
  /** Gives Keycloak's account service, with the person signed in. */
  open: () => Promise<AccountService>;
}) {
  const words = useMemo(() => wordsOf(kcContext), [kcContext]);
  const { say } = words;
  const first = useRef<HTMLDivElement>(null);
  const title = useRef<HTMLHeadingElement>(null);
  const [state, setState] = useState<State>(null);
  // The page goes to a step of the sign-in pages: the first capsule holds what it has.
  const [leaving, setLeaving] = useState(false);
  // Keycloak refused a change of the language because of a detail: the details say which.
  const [blocked, setBlocked] = useState<Refused | null>(null);
  const cursor = useCursor({ busy: state === null || leaving });

  useEffect(() => {
    let here = true;
    ask(open, (found) => {
      if (here) setState(found);
    });
    return () => {
      here = false;
    };
  }, [open]);

  useEffect(() => {
    document.title = `${say("title")} · Kodosi`;
  }, [say]);

  useLayoutEffect(() => {
    const leavePage = () => handOver(first.current, cursor.current?.box() ?? null, "");
    // A page that the browser kept and shows again has old facts and an old session.
    const back = (event: PageTransitionEvent) => {
      if (event.persisted) location.reload();
    };
    window.addEventListener("pagehide", leavePage);
    window.addEventListener("pageshow", back);
    return () => {
      window.removeEventListener("pagehide", leavePage);
      window.removeEventListener("pageshow", back);
    };
  }, [cursor]);

  const loaded = state !== null && state !== "unreachable" ? state : null;
  const came = loaded?.service.came ?? null;

  // A page with no capsule takes away what index.html drew of the page before.
  useLayoutEffect(() => {
    if (state === "unreachable") dropDrawn();
  }, [state]);

  useEffect(() => {
    if (!leaving) return;
    // A step that the browser never opens gives the page back, after the time that the sign-in
    // pages wait.
    const timer = window.setTimeout(() => setLeaving(false), 20_000);
    return () => clearTimeout(timer);
  }, [leaving]);

  /** Changes one fact of the page, from what the page has at that moment. */
  const change = <Name extends keyof Facts>(name: Name, to: (now: Facts[Name]) => Facts[Name]) =>
    setState((now) =>
      now && now !== "unreachable"
        ? { ...now, facts: { ...now.facts, [name]: to(now.facts[name]) } }
        : now,
    );

  const act = (action: string) => {
    // A key that the person holds presses a row many times: the page leaves one time.
    if (!loaded || leaving) return;
    setLeaving(true);
    loaded.service.act(action);
  };

  const person = loaded?.facts.person;
  const fullName = person
    ? [detail(person, "firstName"), detail(person, "lastName")].filter(Boolean).join(" ")
    : "";
  // A realm where the email is the name of the sign-in has no username of its own.
  const handle =
    person && !kcContext.realm.registrationEmailAsUsername && person.username !== person.email
      ? person.username
      : "";
  const name = fullName || (handle ? `@${handle}` : (person?.email ?? ""));
  // The chooser is there when the page can keep the language that a person chooses.
  const languages =
    loaded && kcContext.realm.isInternationalizationEnabled
      ? Object.keys(kcContext.supportedLocales ?? {})
      : [];

  return (
    <div className="flex min-h-dvh flex-col">
      <main
        className={cn(
          "mx-auto flex w-full max-w-[520px] flex-col items-center px-4 pt-(--stage-top) pb-10",
          !carried() && "rise",
        )}
      >
        {/* The room of the mark, which index.html draws. */}
        <div aria-hidden className="h-[calc(var(--logo-size)*1.5)]" />
        {state === "unreachable" ? (
          <>
            <h1 className="mt-8 text-center font-mono text-title font-medium text-ink">
              {say("title")}
            </h1>
            <div role="alert" className="w-full">
              <Note tone="danger" text={say("unreachable")} />
            </div>
            <div className="mt-5 w-full max-w-[260px]">
              <Button
                onClick={() => {
                  setState(null);
                  ask(open, setState);
                }}
              >
                {say("tryAgain")}
              </Button>
            </div>
          </>
        ) : (
          <>
            {/* The head keeps its room while Keycloak answers, so nothing moves when it comes. */}
            <div
              className={cn(
                "mt-8 flex w-full flex-col items-center text-center",
                loaded ? "resolve" : "invisible",
              )}
            >
              <Avatar name={fullName || handle || name} size={56} />
              <h1
                ref={title}
                tabIndex={-1}
                className="mt-3.5 max-w-full font-mono text-title font-medium text-balance break-words text-ink outline-none"
              >
                {name || " "}
              </h1>
              <p className="mt-1 max-w-full truncate font-mono text-footnote text-ink-muted">
                {fullName && handle ? `@${handle}` : " "}
              </p>
            </div>
            <div className="mt-6 w-full">
              <SignIn
                words={words}
                shell={first}
                ways={loaded?.facts.ways ?? null}
                act={act}
                leaving={leaving}
                note={came?.status === "error" ? say("failed") : null}
              />
              {loaded && (
                <>
                  <LinkedAccounts
                    words={words}
                    service={loaded.service}
                    linked={loaded.facts.linked}
                    act={act}
                    onChange={(to) => change("linked", to)}
                    order={0}
                  />
                  <SignedIn
                    words={words}
                    service={loaded.service}
                    sessions={loaded.facts.sessions}
                    onChange={(to) => change("sessions", to)}
                    order={1}
                  />
                  <Apps
                    words={words}
                    service={loaded.service}
                    programs={loaded.facts.programs}
                    onChange={(to) => change("programs", to)}
                    home={title}
                    order={1}
                  />
                  <Details
                    words={words}
                    service={loaded.service}
                    person={loaded.facts.person}
                    act={act}
                    blocked={blocked}
                    onSaved={(values) => {
                      setBlocked(null);
                      change("person", (now) => ({
                        ...now,
                        ...(values.email === undefined ? {} : { email: values.email }),
                        ...(values.username === undefined ? {} : { username: values.username }),
                        fields: now.fields.map((one) => {
                          const value = values[one.name];
                          return value === undefined ? one : { ...one, value };
                        }),
                      }));
                    }}
                    order={2}
                  />
                  <div className="rise mt-9 flex flex-wrap items-center justify-center gap-x-6 gap-y-2 [animation-delay:210ms]">
                    {kcContext.referrer_uri && (
                      <QuietLink href={kcContext.referrer_uri}>
                        {say("backTo", words.named(kcContext.referrerName ?? "") || "Kodosi")}
                      </QuietLink>
                    )}
                    <button
                      type="button"
                      className={quietLink}
                      onClick={() => loaded.service.signOut()}
                    >
                      {say("signOut")}
                    </button>
                    {kcContext.deleteAccountAllowed && (
                      <button
                        type="button"
                        className={cn(quietLink, "text-danger")}
                        onClick={() => act("delete_account")}
                      >
                        {say("deleteAccount")}
                      </button>
                    )}
                  </div>
                </>
              )}
            </div>
          </>
        )}
      </main>
      <footer className="mt-auto flex justify-center px-4 pb-[max(1.5rem,env(safe-area-inset-bottom))]">
        <Languages
          label={say("language")}
          current={kcContext.locale}
          languages={languages.map((tag) => ({ tag, label: words.languageName(tag) }))}
          onChoose={(tag) =>
            loaded?.service.setLanguage(tag).catch((error: unknown) => {
              // Keycloak keeps the language with the details. When a detail stops it, the
              // details say which one.
              if (error instanceof Refused) setBlocked(error);
              throw error;
            })
          }
        />
      </footer>
    </div>
  );
}
