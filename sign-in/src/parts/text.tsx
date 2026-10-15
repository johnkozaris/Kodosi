import { kcSanitize } from "keycloakify/lib/kcSanitize";
import { X } from "lucide-react";
import type { ComponentProps, ReactNode } from "react";

import { buttonClass } from "./button";
import { cn } from "./cn";
import { Avatar } from "./person";

export function plain(html: string): string {
  return new DOMParser().parseFromString(html, "text/html").body.textContent ?? "";
}

export function safeHtml(html: string): string {
  const page = new DOMParser().parseFromString(kcSanitize(html), "text/html");
  for (const link of page.querySelectorAll("a[href]")) {
    link.setAttribute("target", "_blank");
    link.setAttribute("rel", "noreferrer");
  }
  return page.body.innerHTML;
}

export const NOTE_ID = "page-note";

export function Note({
  tone,
  text,
  id = NOTE_ID,
}: {
  tone: "danger" | "caution" | "plain";
  text: string;
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
