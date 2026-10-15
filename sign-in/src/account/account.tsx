import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { dropDrawn, handOver } from "../frame/carry";
import { Page } from "../frame/page";
import { useCursor } from "../frame/use-cursor";
import { cn } from "../parts/cn";
import { Languages } from "../parts/languages";
import { Legal } from "../parts/legal";
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

type State = { service: AccountService; facts: Facts } | "unreachable" | null;

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

export function Account({
  kcContext,
  open,
}: {
  kcContext: KcContext;
  open: () => Promise<AccountService>;
}) {
  const words = useMemo(() => wordsOf(kcContext), [kcContext]);
  const { say } = words;
  const first = useRef<HTMLDivElement>(null);
  const title = useRef<HTMLHeadingElement>(null);
  const [state, setState] = useState<State>(null);
  const [leaving, setLeaving] = useState(false);
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

  useLayoutEffect(() => {
    if (state === "unreachable") dropDrawn();
  }, [state]);

  useEffect(() => {
    if (!leaving) return;
    const timer = window.setTimeout(() => setLeaving(false), 20_000);
    return () => clearTimeout(timer);
  }, [leaving]);

  const change = <Name extends keyof Facts>(name: Name, to: (now: Facts[Name]) => Facts[Name]) =>
    setState((now) =>
      now && now !== "unreachable"
        ? { ...now, facts: { ...now.facts, [name]: to(now.facts[name]) } }
        : now,
    );

  const act = (action: string) => {
    if (!loaded || leaving) return;
    setLeaving(true);
    loaded.service.act(action);
  };

  const person = loaded?.facts.person;
  const fullName = person
    ? [detail(person, "firstName"), detail(person, "lastName")].filter(Boolean).join(" ")
    : "";
  const handle =
    person && !kcContext.realm.registrationEmailAsUsername && person.username !== person.email
      ? person.username
      : "";
  const name = fullName || (handle ? `@${handle}` : (person?.email ?? ""));
  const languages =
    loaded && kcContext.realm.isInternationalizationEnabled
      ? Object.keys(kcContext.supportedLocales ?? {})
      : [];

  return (
    <Page
      wide
      foot={
        <>
          <Languages
            label={say("language")}
            current={kcContext.locale}
            languages={languages.map((tag) => ({ tag, label: words.languageName(tag) }))}
            onChoose={(tag) =>
              loaded?.service.setLanguage(tag).catch((error: unknown) => {
                if (error instanceof Refused) setBlocked(error);
                throw error;
              })
            }
          />
          <Legal
            links={[
              { label: say("terms"), href: kcContext.properties.KODOSI_TERMS_URL },
              { label: say("privacy"), href: kcContext.properties.KODOSI_PRIVACY_URL },
            ]}
          />
        </>
      }
    >
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
                </div>
                {kcContext.deleteAccountAllowed && (
                  <div className="rise mt-10 flex justify-center [animation-delay:280ms]">
                    <button
                      type="button"
                      className={cn(quietLink, "text-danger")}
                      onClick={() => act("delete_account")}
                    >
                      {say("deleteAccount")}
                    </button>
                  </div>
                )}
              </>
            )}
          </div>
        </>
      )}
    </Page>
  );
}
