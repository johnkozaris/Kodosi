import {
  Check,
  ChevronRight,
  Fingerprint,
  KeyRound,
  LifeBuoy,
  Monitor,
  Plus,
  Smartphone,
  SquareTerminal,
  Usb,
  X,
} from "lucide-react";
import {
  type ReactNode,
  type Ref,
  type RefObject,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";

import { Shell } from "../frame/shell";
import { BrandMark } from "../parts/brands";
import { cn } from "../parts/cn";
import { ActionRow, ItemRow, RowAction, RowIcon, rule } from "../parts/rows";
import { Note, quietLink } from "../parts/text";
import { IconTile, type Tint } from "../parts/tile";
import {
  type AccountService,
  type Field,
  type Linked,
  type Person,
  type Program,
  Refused,
  type Session,
  type Way,
} from "./service";
import type { WordKey, Words } from "./words";

/** A change to a list of the page, from the list as it is at that moment. */
type Change<T> = (change: (now: T[]) => T[]) => void;

/** A group of the page: its name, and one capsule with its rows. */
function Group({
  title,
  heading,
  shell,
  first,
  busy,
  order = 0,
  note,
  noteId,
  under,
  children,
}: {
  title: string;
  /** The name of the group takes the focus when the row that had it goes away. */
  heading?: Ref<HTMLHeadingElement>;
  shell?: Ref<HTMLDivElement> | undefined;
  /** The capsule that carries on from the sign-in pages. */
  first?: boolean;
  busy?: boolean;
  /** Its place among the groups that come after the first: each comes a moment later. */
  order?: number;
  /** What went wrong with the last change, in one line. */
  note?: string | null | undefined;
  /** The id of that line, when a field of the group names it. */
  noteId?: string;
  /** Under the capsule: a quiet action for the whole group. */
  under?: ReactNode;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <section
      aria-labelledby={id}
      aria-busy={busy || undefined}
      className={cn("w-full", !first && "rise mt-7")}
      style={first ? undefined : { animationDelay: `${order * 70}ms` }}
    >
      <h2
        id={id}
        ref={heading}
        tabIndex={-1}
        className="px-[18px] pb-2 text-footnote font-semibold text-ink-muted outline-none"
      >
        {title}
      </h2>
      <Shell ref={shell} carries={!!first} grows busy={!!busy}>
        {children}
      </Shell>
      {/* The box is there from the start, so a screen reader says a line that comes into it. */}
      <div aria-live="polite">
        {note && <Note tone="danger" text={note} id={noteId ?? `${id}-note`} />}
      </div>
      {under && <div className="mt-3 flex justify-center">{under}</div>}
    </section>
  );
}

/**
 * One change of a group: the group is busy while Keycloak answers, and one line says a change
 * that did not work.
 */
function useChange(words: Words) {
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const run = async (change: () => Promise<void>) => {
    setBusy(true);
    setNote(null);
    try {
      await change();
    } catch (error) {
      const said = error instanceof Refused ? error.problems[0] : undefined;
      setNote(said ? words.problem(said) : words.say("failed"));
    } finally {
      setBusy(false);
    }
  };
  return { busy, note, run };
}

/** The rows of a capsule before Keycloak answered: their places, with nothing in them. */
function Waiting() {
  return (
    <div aria-hidden>
      {[112, 148, 96].map((width) => (
        <div
          key={width}
          className={cn(rule, "flex h-14 items-center gap-3 pr-4 pl-4 before:left-[58px]")}
        >
          <span className="squircle skeleton size-[30px]" />
          <span className="skeleton h-2.5 rounded-full" style={{ width }} />
        </div>
      ))}
    </div>
  );
}

/** How a way of sign-in shows: its tile, its name, and the words of the row that adds one. */
const LOOKS: Record<
  string,
  {
    icon: ReactNode;
    tint: Tint;
    name: WordKey;
    add?: WordKey;
    hint?: WordKey;
    /** The words of the action that makes a new one in place of the one that the person has. */
    again?: WordKey;
    many: boolean;
  }
> = {
  password: {
    icon: <KeyRound />,
    tint: "shell",
    name: "password",
    add: "passwordSet",
    many: false,
  },
  "webauthn-passwordless": {
    icon: <Fingerprint />,
    tint: "copper",
    name: "passkey",
    add: "passkeyAdd",
    hint: "passkeyHint",
    many: true,
  },
  otp: {
    icon: <Smartphone />,
    tint: "violet",
    name: "authenticator",
    add: "authenticatorAdd",
    hint: "authenticatorHint",
    many: true,
  },
  webauthn: {
    icon: <Usb />,
    tint: "graphite",
    name: "securityKey",
    add: "securityKeyAdd",
    many: true,
  },
  "recovery-authn-codes": {
    icon: <LifeBuoy />,
    tint: "teal",
    name: "recovery",
    hint: "recoveryHint",
    again: "recoveryAgain",
    many: false,
  },
};
const ORDER = Object.keys(LOOKS);

/**
 * The ways of sign-in that the realm offers, with what the person has of each. Each change is a
 * step of the sign-in pages: Keycloak asks for the person's proof there.
 */
export function SignIn({
  words,
  ways,
  act,
  leaving,
  shell,
  note,
}: {
  words: Words;
  /** Null while Keycloak has not answered. */
  ways: Way[] | null;
  act: (action: string) => void;
  /** The page goes to a step of the sign-in pages. */
  leaving: boolean;
  shell: Ref<HTMLDivElement>;
  note?: string | null;
}) {
  const { say, day } = words;
  const rows: ReactNode[] = [];
  const known = (ways ?? [])
    .filter((way) => way.type in LOOKS)
    .toSorted((a, b) => ORDER.indexOf(a.type) - ORDER.indexOf(b.type));
  for (const way of known) {
    const look = LOOKS[way.type] as (typeof LOOKS)[string];
    const tile = <IconTile tint={look.tint} icon={look.icon} />;
    const name = say(look.name);
    const one = way.held[0];
    if (!look.many) {
      const step = one ? (way.update ?? way.create) : way.create;
      const detail = !one
        ? step && look.hint && say(look.hint)
        : one.left !== undefined && one.total !== undefined
          ? say("recoveryLeft", one.left, one.total)
          : one.created
            ? say(way.type === "password" ? "changed" : "added", day(one.created))
            : undefined;
      // The words that add one are for a row that can add it.
      const title = !one && step && look.add ? say(look.add) : name;
      // A thing that the person can remove has its actions at the end of the row.
      if (one && way.removable) {
        rows.push(
          <ItemRow
            key={way.type}
            tile={tile}
            title={title}
            detail={detail}
            end={
              <>
                {step && look.again && (
                  <RowAction onClick={() => act(step)}>{say(look.again)}</RowAction>
                )}
                <RowIcon
                  label={say("remove", name)}
                  onClick={() => act(`delete_credential:${one.id}`)}
                >
                  <X strokeWidth={2.2} aria-hidden />
                </RowIcon>
              </>
            }
          />,
        );
        continue;
      }
      rows.push(
        step ? (
          <ActionRow
            key={way.type}
            type="button"
            tile={tile}
            title={title}
            detail={detail}
            onClick={() => act(step)}
          />
        ) : (
          <ItemRow key={way.type} tile={tile} title={title} detail={detail} />
        ),
      );
      continue;
    }
    for (const held of way.held) {
      // A thing with its own name says its kind in the small line.
      const added = held.created ? say("added", day(held.created)) : "";
      rows.push(
        <ItemRow
          key={held.id}
          tile={tile}
          title={held.label ?? name}
          detail={held.label ? [name, added].filter(Boolean).join(". ") : added || undefined}
          end={
            way.removable && (
              <RowIcon
                label={say("remove", held.label ? `${held.label} (${name})` : name)}
                onClick={() => act(`delete_credential:${held.id}`)}
              >
                <X strokeWidth={2.2} aria-hidden />
              </RowIcon>
            )
          }
        />,
      );
    }
    const create = way.create;
    if (create && look.add) {
      rows.push(
        <ActionRow
          key={`${way.type}-add`}
          type="button"
          // The tile of the kind is on what the person has. One more of it is a slot to fill.
          tile={
            way.held.length ? (
              <span
                aria-hidden
                className="squircle grid size-[30px] shrink-0 place-items-center border border-dashed border-ink-faint text-ink-muted"
              >
                <Plus className="size-4" strokeWidth={2.2} />
              </span>
            ) : (
              tile
            )
          }
          title={say(look.add)}
          detail={way.held.length === 0 && look.hint ? say(look.hint) : undefined}
          onClick={() => act(create)}
        />,
      );
    }
  }
  // A realm with ways of sign-in that the page does not know has no such group.
  if (ways !== null && !rows.length) return null;
  return (
    <Group title={say("signIn")} shell={shell} first busy={ways === null || leaving} note={note}>
      {ways === null ? <Waiting /> : rows}
    </Group>
  );
}

/** The mark of a sign-in service, in a well: the mark keeps its own colours. */
function BrandTile({ alias, providerId }: { alias: string; providerId?: string | undefined }) {
  return (
    <span
      aria-hidden
      className="well squircle grid size-[30px] shrink-0 place-items-center text-ink"
    >
      <BrandMark alias={alias} {...(providerId ? { providerId } : {})} />
    </span>
  );
}

/** The sign-in services of other companies that the realm lets a person join to the account. */
export function LinkedAccounts({
  words,
  service,
  linked,
  act,
  onChange,
  order,
}: {
  words: Words;
  service: AccountService;
  linked: Linked[];
  /** A link is a step of the sign-in pages: the other service asks who the person is there. */
  act: (action: string) => void;
  onChange: Change<Linked>;
  order: number;
}) {
  const { say } = words;
  const { busy, note, run } = useChange(words);
  if (!linked.length) return null;

  const unlink = (one: Linked) =>
    run(async () => {
      await service.unlink(one.alias);
      onChange((now) =>
        now.map((each) => {
          if (each.alias !== one.alias) return each;
          const { as: _, ...rest } = each;
          return { ...rest, connected: false };
        }),
      );
    });

  return (
    <Group title={say("linked")} order={order} busy={busy} note={note}>
      {linked.map((one) => (
        <ItemRow
          key={one.alias}
          tile={<BrandTile alias={one.alias} providerId={one.providerId} />}
          title={one.name}
          // The person's name at that service, on one line.
          detail={
            one.connected && one.as ? <span className="block truncate">{one.as}</span> : undefined
          }
          end={
            <RowAction
              aria-label={say(one.connected ? "remove" : "addNamed", one.name)}
              disabled={busy}
              onClick={() => (one.connected ? void unlink(one) : act(`idp_link:${one.alias}`))}
            >
              {say(one.connected ? "removeOne" : "addOne")}
            </RowAction>
          }
        />
      ))}
    </Group>
  );
}

function browserName({ say }: Words, session: Session): string {
  // Keycloak has the names that the makers gave. A person knows a Mac as macOS, and the Safari
  // of a phone as Safari.
  const os = session.os.replace(/^Mac OS X$/i, "macOS");
  const browser = session.browser.replace(/^Mobile | Mobile$/g, "");
  if (browser && os) return say("browserOn", browser, os);
  return browser || os || say("browserUnknown");
}

/**
 * Where the person is signed in: each browser, with the programs that they signed in to from it.
 * A browser that is not theirs gets a sign-out from here.
 */
export function SignedIn({
  words,
  service,
  sessions,
  onChange,
  order,
}: {
  words: Words;
  service: AccountService;
  sessions: Session[];
  onChange: Change<Session>;
  order: number;
}) {
  const { say } = words;
  const heading = useRef<HTMLHeadingElement>(null);
  const { busy, note, run } = useChange(words);
  const others = sessions.filter((session) => !session.current);

  const end = (work: () => Promise<void>, keep: (session: Session) => boolean) =>
    run(async () => {
      await work();
      onChange((now) => now.filter(keep));
      // The row that had the focus is gone.
      heading.current?.focus();
    });

  return (
    <Group
      title={say("signedIn")}
      heading={heading}
      order={order}
      busy={busy}
      note={note}
      under={
        others.length > 1 && (
          <button
            type="button"
            className={quietLink}
            disabled={busy}
            onClick={() =>
              void end(
                () => service.endOtherSessions(),
                (session) => session.current,
              )
            }
          >
            {say("signOutOthers")}
          </button>
        )
      }
    >
      {sessions.map((session) => {
        const name = browserName(words, session);
        const when = session.current ? say("now") : words.ago(session.lastAccess);
        return (
          <ItemRow
            key={session.id}
            tile={
              <IconTile
                tint={session.current ? "green" : "graphite"}
                icon={session.mobile ? <Smartphone /> : <Monitor />}
              />
            }
            title={name}
            detail={[...session.programs, when].join(", ")}
            end={
              session.current ? (
                <span className="mr-1.5 rounded-full bg-ready-soft px-2 py-[3px] text-caption font-medium text-ready">
                  {say("thisBrowser")}
                </span>
              ) : (
                <RowAction
                  aria-label={say("signOutOf", name)}
                  disabled={busy}
                  onClick={() =>
                    void end(
                      () => service.endSession(session.id),
                      (one) => one.id !== session.id,
                    )
                  }
                >
                  {say("signOutOne")}
                </RowAction>
              )
            }
          />
        );
      })}
    </Group>
  );
}

/**
 * The programs that the person let use the account: the Kodosi app, for example, which stays
 * signed in on each computer. The person can take that back, and each computer then asks for a
 * new sign-in.
 */
export function Apps({
  words,
  service,
  programs,
  onChange,
  home,
  order,
}: {
  words: Words;
  service: AccountService;
  programs: Program[];
  onChange: Change<Program>;
  /** What takes the focus when the last program goes, and the group with it. */
  home: RefObject<HTMLElement | null>;
  order: number;
}) {
  const { say } = words;
  const heading = useRef<HTMLHeadingElement>(null);
  const { busy, note, run } = useChange(words);
  if (!programs.length && !note) return null;

  const revoke = (clientId: string) =>
    run(async () => {
      await service.revoke(clientId);
      onChange((now) => now.filter((one) => one.clientId !== clientId));
      (programs.length > 1 ? heading : home).current?.focus();
    });

  return (
    <Group title={say("apps")} heading={heading} order={order} busy={busy} note={note}>
      {programs.map((one) => {
        const name = words.named(one.name);
        return (
          <ItemRow
            key={one.clientId}
            tile={<IconTile tint="shell" icon={<SquareTerminal />} />}
            title={name}
            detail={one.stays ? say("staysSignedIn") : undefined}
            end={
              <RowAction
                aria-label={say(one.stays ? "signOutOf" : "remove", name)}
                disabled={busy}
                onClick={() => void revoke(one.clientId)}
              >
                {say(one.stays ? "signOutOne" : "removeOne")}
              </RowAction>
            }
          />
        );
      })}
    </Group>
  );
}

/** What Keycloak refused: the rows that it names, and its sentence. */
interface Refusal {
  fields: string[];
  text: string;
}

/** One detail that the person can change. The row saves when the person leaves it. */
function DetailRow({
  words,
  field,
  value,
  saved,
  refused,
  unsaved,
  noteId,
  onChange,
  onLeave,
  onCancel,
}: {
  words: Words;
  field: Field;
  /** What the row shows: what the person typed, or what Keycloak has. */
  value: string;
  /** Keycloak took the value a moment ago. */
  saved: boolean;
  /** Keycloak refused the details because of this row. */
  refused: boolean;
  /** The person typed a value here that Keycloak does not have, because it refused the details. */
  unsaved: boolean;
  /** The id of the line that says what Keycloak refused. */
  noteId: string;
  onChange: (value: string) => void;
  onLeave: () => void;
  onCancel: () => void;
}) {
  const id = useId();
  return (
    <div className={cn(rule, "flex min-h-[52px] items-center gap-3 py-2 pr-3 pl-[18px]")}>
      <label
        htmlFor={id}
        className={cn("shrink-0 text-[14.5px] sm:w-32", refused ? "text-danger" : "text-ink")}
      >
        {words.label(field.name, field.label)}
      </label>
      <div className="flex min-w-0 flex-1 items-center justify-end gap-2">
        {saved && (
          <output className="pop flex shrink-0 items-center gap-1 text-caption font-medium text-ready">
            <Check className="size-3.5" strokeWidth={2.6} aria-hidden />
            {words.say("saved")}
          </output>
        )}
        {unsaved && (
          <span className="shrink-0 text-caption font-medium text-danger">
            {words.say("notSaved")}
          </span>
        )}
        <input
          id={id}
          type={field.name === "email" ? "email" : "text"}
          value={value}
          required={field.required}
          aria-invalid={refused || undefined}
          aria-describedby={refused ? noteId : undefined}
          autoComplete={
            field.name === "firstName"
              ? "given-name"
              : field.name === "lastName"
                ? "family-name"
                : field.name === "email"
                  ? "email"
                  : "off"
          }
          autoCapitalize={field.name === "username" || field.name === "email" ? "none" : undefined}
          spellCheck={false}
          onChange={(event) => onChange(event.target.value)}
          // A window that loses the focus takes it from the field too. The person did not leave
          // the row then, so half of a value is not saved.
          onBlur={() => {
            if (document.hasFocus()) onLeave();
          }}
          onKeyDown={(event) => {
            // A key that ends a word of an input method is not a key for the row. Safari tells it
            // with the key code 229 only.
            if (event.nativeEvent.isComposing || event.keyCode === 229) return;
            if (event.key === "Enter") event.currentTarget.blur();
            if (event.key === "Escape") onCancel();
          }}
          // The field that has the typing is a well with the copper line about it.
          className="h-9 w-full min-w-0 rounded-md bg-transparent px-2.5 text-right font-mono text-[14px] text-ink-muted outline-none transition-[background-color,box-shadow,color] duration-200 hover:bg-well/60 focus:bg-well focus:text-left focus:text-ink focus:shadow-[var(--e-well),0_0_0_2px_color-mix(in_srgb,var(--accent)_55%,transparent)] sm:max-w-[290px] pointer-coarse:h-11 pointer-coarse:text-[16px]"
        />
      </div>
    </div>
  );
}

/** The details of the person that the realm lets them change. */
export function Details({
  words,
  service,
  person,
  act,
  blocked,
  onSaved,
  order,
}: {
  words: Words;
  service: AccountService;
  person: Person;
  act: (action: string) => void;
  /** Keycloak refused a change from a different part of the page because of a detail. */
  blocked: Refused | null;
  onSaved: (values: Record<string, string>) => void;
  order: number;
}) {
  // What the person typed in a row and Keycloak does not have yet.
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  // What Keycloak refused at the last save from here. One line under the capsule says it.
  const [refusal, setRefusal] = useState<Refusal | null>(null);
  // The rows that say "Saved" for a moment.
  const [saved, setSaved] = useState<string[]>([]);
  const flash = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(flash.current), []);
  const noteId = useId();

  const open = person.fields.filter((field) => !field.readOnly && field.name !== "locale");
  const email = person.emailStep && !open.some((field) => field.name === "email");
  if (!open.length && !email) return null;

  /** The rows with a typed value that Keycloak does not have. */
  const changed = (typed: Record<string, string>) =>
    Object.fromEntries(
      open.flatMap((field) => {
        const value = typed[field.name]?.trim();
        return value !== undefined && value !== field.value.trim() ? [[field.name, value]] : [];
      }),
    );
  const waiting = changed(drafts);
  /** The rows of this group that Keycloak names. */
  const named = (error: Refused) =>
    error.fields.filter((name) => open.some((field) => field.name === name));
  const sentence = (error: Refused) =>
    error.problems[0] ? words.problem(error.problems[0]) : words.say("failed");
  const problem =
    refusal ??
    (blocked && named(blocked).length ? { fields: named(blocked), text: sentence(blocked) } : null);

  const leave = async (row: string) => {
    if (!Object.keys(waiting).length) {
      setDrafts({});
      setRefusal(null);
      return;
    }
    try {
      // Keycloak checks all details at each save, so each row with a new value goes together:
      // two rows that it needs can not go one after the other.
      await service.save(waiting);
    } catch (error) {
      const refused = error instanceof Refused ? error : null;
      const rows = refused ? named(refused) : [];
      setRefusal({
        fields: rows.length ? rows : [row],
        text: refused ? sentence(refused) : words.say("failed"),
      });
      return;
    }
    onSaved(waiting);
    // A row keeps what the person typed while the save was on its way.
    setDrafts((now) =>
      Object.fromEntries(
        Object.entries(now).filter(([name, typed]) => typed.trim() !== waiting[name]),
      ),
    );
    setRefusal(null);
    setSaved(Object.keys(waiting));
    window.clearTimeout(flash.current);
    flash.current = window.setTimeout(() => setSaved([]), 1600);
  };

  return (
    <Group title={words.say("details")} order={order} note={problem?.text} noteId={noteId}>
      {open.map((field) => (
        <DetailRow
          key={field.name}
          words={words}
          field={field}
          value={drafts[field.name] ?? field.value}
          saved={saved.includes(field.name)}
          refused={!!problem?.fields.includes(field.name)}
          unsaved={!!refusal && field.name in waiting}
          noteId={noteId}
          onChange={(value) => setDrafts((now) => ({ ...now, [field.name]: value }))}
          onLeave={() => void leave(field.name)}
          onCancel={() => {
            setDrafts((now) => {
              const { [field.name]: _, ...others } = now;
              return others;
            });
            // The line about this row goes with what the person typed in it.
            setRefusal((now) => (now?.fields.every((name) => name === field.name) ? null : now));
          }}
        />
      ))}
      {email && (
        <button
          type="button"
          onClick={() => act("UPDATE_EMAIL")}
          className={cn(
            rule,
            "group flex min-h-[52px] w-full items-center gap-3 py-2 pr-4 pl-[18px] text-left transition-colors duration-200 hover:bg-lifted focus-visible:-outline-offset-2 focus-visible:first:rounded-t-[20px] focus-visible:last:rounded-b-[20px]",
          )}
        >
          <span className="shrink-0 text-[14.5px] text-ink sm:w-32">{words.say("email")}</span>
          <span className="min-w-0 flex-1 truncate text-right font-mono text-[14px] text-ink-muted">
            {person.email}
          </span>
          <ChevronRight
            className="size-4 shrink-0 text-ink-faint transition-transform duration-200 ease-out group-hover:translate-x-0.5"
            aria-hidden
          />
        </button>
      )}
    </Group>
  );
}
