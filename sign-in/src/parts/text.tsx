import { kcSanitize } from "keycloakify/lib/kcSanitize";
import { X } from "lucide-react";
import type { ComponentProps, ReactNode } from "react";

import { buttonClass } from "./button";
import { cn } from "./cn";
import { Avatar } from "./person";

/** Keycloak's words can hold markup. A page shows the words alone, so no markup of a message runs. */
export function plain(html: string): string {
  return new DOMParser().parseFromString(html, "text/html").body.textContent ?? "";
}

/**
 * The realm's own markup, as its terms, cleaned. Its links open in a new tab, so a form that the
 * person fills stays as it is.
 */
export function safeHtml(html: string): string {
  const page = new DOMParser().parseFromString(kcSanitize(html), "text/html");
  for (const link of page.querySelectorAll("a[href]")) {
    link.setAttribute("target", "_blank");
    link.setAttribute("rel", "noreferrer");
  }
  return page.body.innerHTML;
}

/** The id of the page's note. A field that Keycloak refused names it, so a screen reader says the note with the field. */
export const NOTE_ID = "page-note";

/** One line of what Keycloak says, under the capsule. The dot carries the tone, never an alarm. */
export function Note({
  tone,
  text,
  id = NOTE_ID,
}: {
  tone: "danger" | "caution" | "plain";
  text: string;
  /** A page with more than one capsule gives each note its own id. */
  id?: string;
}) {
  return (
    <p
      id={id}
      className={cn(
        "rise mx-auto mt-3.5 max-w-[44ch] text-center text-footnote text-balance",
        tone === "danger" ? "text-danger" : tone === "caution" ? "text-ink" : "text-ink-muted",
      )}
    >
      <span
        aria-hidden
        className={cn(
          "mr-2 inline-block size-1.5 rounded-full align-middle",
          tone === "danger" ? "bg-danger" : tone === "caution" ? "bg-caution" : "bg-ink-faint",
        )}
      />
      {plain(text)}
    </p>
  );
}

/**
 * Who signs in, when Keycloak knows it already: the first row of the capsule, and the step's own
 * rows come under it. A password manager reads the name from here.
 */
export function IdentityRow({
  name,
  restart,
  restartLabel,
}: {
  name: string;
  restart: string;
  restartLabel: string;
}) {
  return (
    <div className="relative flex h-12 items-center gap-2.5 pr-2.5 pl-4 after:absolute after:right-0 after:bottom-0 after:left-[54px] after:h-px after:bg-hairline/60">
      <Avatar name={name} size={28} />
      <input
        readOnly
        tabIndex={-1}
        aria-label={name}
        value={name}
        autoComplete="username"
        className="min-w-0 flex-1 truncate bg-transparent font-mono text-[14px] text-ink-muted outline-none pointer-coarse:text-[16px]"
      />
      <a
        href={restart}
        id="reset-login"
        aria-label={restartLabel}
        title={restartLabel}
        className="grid size-8 shrink-0 place-items-center rounded-full text-ink-muted transition-colors duration-200 hover:bg-well/70 hover:text-ink focus-visible:-outline-offset-2 pointer-coarse:size-11"
      >
        <X className="size-[15px]" strokeWidth={2.2} aria-hidden />
      </a>
    </div>
  );
}

/** A quiet way to a different step, as a link or as the button of a form. Under a finger it has
    the height that a finger needs. */
export const quietLink =
  "rounded-md text-footnote font-medium text-accent-strong underline-offset-3 hover:underline disabled:pointer-events-none disabled:opacity-45 pointer-coarse:inline-flex pointer-coarse:min-h-11 pointer-coarse:items-center pointer-coarse:px-2";

export function QuietLink({ className, children, ...props }: ComponentProps<"a">) {
  return (
    <a className={cn(quietLink, className)} {...props}>
      {children}
    </a>
  );
}

const way = "h-11 w-full min-w-0 gap-2.5 px-3 text-[14.5px]";

/** A different way in: a passkey, or the sign-in of a different company. */
export function Way({
  icon,
  children,
  href,
  mark,
  ...props
}: {
  icon?: ReactNode;
  children: ReactNode;
  href?: string;
  /** The way that this browser used the last time: the copper dot of the app says so. */
  mark?: string | undefined;
} & Omit<ComponentProps<"button">, "children">) {
  const inside = (
    <>
      {icon}
      <span className="truncate">{children}</span>
      {mark && (
        <span title={mark} className="size-1.5 shrink-0 rounded-full bg-accent">
          <span className="sr-only">{mark}</span>
        </span>
      )}
    </>
  );
  const look = buttonClass({ variant: "secondary", className: way });
  if (href) {
    return (
      <a
        href={href}
        id={props.id}
        aria-label={props["aria-label"]}
        onClick={props.onClick as never}
        className={look}
      >
        {inside}
      </a>
    );
  }
  return (
    <button type="button" {...props} className={look}>
      {inside}
    </button>
  );
}
