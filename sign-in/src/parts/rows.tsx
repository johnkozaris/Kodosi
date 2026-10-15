import { ArrowRight, ChevronRight, Eye, EyeOff } from "lucide-react";
import {
  type ComponentProps,
  type KeyboardEvent,
  type ReactNode,
  type Ref,
  useEffect,
  useId,
  useImperativeHandle,
  useRef,
  useState,
} from "react";

import { cn } from "./cn";
import { NOTE_ID } from "./text";

export const rule =
  "relative before:absolute before:top-0 before:right-0 before:left-[18px] before:h-px before:bg-hairline/60 first:before:hidden";

type FieldProps = Omit<ComponentProps<"input">, "placeholder" | "className" | "ref"> & {
  label: ReactNode;
  end?: ReactNode;
  ref?: Ref<HTMLInputElement>;
  fits?: boolean;
};

export function TextRow({ label, end, id, type, ref, fits = true, ...input }: FieldProps) {
  const own = useId();
  const field = id ?? own;
  const node = useRef<HTMLInputElement>(null);
  useImperativeHandle(ref, () => node.current as HTMLInputElement);
  useEffect(() => {
    node.current?.setCustomValidity(fits ? "" : "not yet");
  }, [fits]);
  const refused = input["aria-invalid"] === true || input["aria-invalid"] === "true";
  // Browsers give the caret position, which the cursor follows, only for a field of text.
  const email = type === "email";
  return (
    <div className={cn(rule, "flex h-14 items-center")}>
      <div className="relative h-full min-w-0 flex-1">
        <input
          ref={node}
          id={field}
          className="row-input"
          placeholder=" "
          aria-describedby={refused ? NOTE_ID : undefined}
          type={email ? "text" : type}
          {...(email ? { inputMode: "email", autoCapitalize: "none", spellCheck: false } : {})}
          {...input}
        />
        <label htmlFor={field} className="row-label">
          {label}
        </label>
      </div>
      {end && <div className="flex shrink-0 items-center gap-1.5 pr-2.5">{end}</div>}
    </div>
  );
}

const rowButton =
  "grid size-9 place-items-center rounded-full text-ink-muted transition-colors duration-200 hover:bg-well/70 hover:text-ink focus-visible:-outline-offset-2 pointer-coarse:size-11";

export function PasswordRow({
  show,
  hide,
  capsLock,
  end,
  onKeyUp,
  ...field
}: FieldProps & {
  show: string;
  hide: string;
  capsLock: string;
}) {
  const [shown, setShown] = useState(false);
  const [caps, setCaps] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const watchCaps = (event: KeyboardEvent<HTMLInputElement>) => {
    setCaps(event.getModifierState("CapsLock"));
    onKeyUp?.(event);
  };
  return (
    <TextRow
      {...field}
      ref={input}
      type={shown ? "text" : "password"}
      autoCapitalize="none"
      spellCheck={false}
      onKeyUp={watchCaps}
      onBlur={(event) => {
        setCaps(false);
        field.onBlur?.(event);
      }}
      end={
        <>
          {caps && (
            <span className="pop rounded-full bg-caution-soft px-2 py-[3px] text-caption font-medium text-caution">
              {capsLock}
            </span>
          )}
          <button
            type="button"
            aria-label={shown ? hide : show}
            aria-pressed={shown}
            onClick={() => {
              setShown((now) => !now);
              requestAnimationFrame(() => input.current?.focus());
            }}
            className={rowButton}
          >
            {shown ? (
              <EyeOff className="size-[17px]" aria-hidden />
            ) : (
              <Eye className="size-[17px]" aria-hidden />
            )}
          </button>
          {end}
        </>
      }
    />
  );
}

export function Go({ label, ...props }: { label: string } & ComponentProps<"button">) {
  return (
    <button
      type="submit"
      aria-label={label}
      title={label}
      {...props}
      className="go grid size-9 shrink-0 place-items-center rounded-full transition-[background-color,color,box-shadow,scale] duration-200 ease-out active:scale-[0.94] pointer-coarse:size-11"
    >
      <ArrowRight className="size-[18px]" strokeWidth={2.4} aria-hidden />
    </button>
  );
}

export function SwitchRow({
  label,
  ...input
}: { label: ReactNode } & Omit<ComponentProps<"input">, "type" | "className">) {
  const own = useId();
  const field = input.id ?? own;
  const refused = input["aria-invalid"] === true || input["aria-invalid"] === "true";
  return (
    <label
      htmlFor={field}
      className={cn(rule, "flex min-h-12 cursor-pointer items-center gap-3 py-2 pr-4 pl-[18px]")}
    >
      <span
        className={cn(
          "flex-1 text-[14.5px] transition-colors duration-200",
          refused ? "text-danger" : "text-ink",
        )}
      >
        {label}
      </span>
      <input
        id={field}
        type="checkbox"
        className="peer sr-only"
        aria-describedby={refused ? NOTE_ID : undefined}
        {...input}
      />
      <span
        aria-hidden
        className="well relative h-[26px] w-[44px] shrink-0 rounded-full transition-colors duration-200 peer-checked:bg-accent peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-accent after:absolute after:top-[3px] after:left-[3px] after:size-5 after:rounded-full after:bg-lifted after:shadow-[0_1px_2px_rgb(0_0_0/0.3)] after:transition-transform after:duration-200 after:ease-out peer-checked:after:translate-x-[18px] dark:after:bg-ink"
      />
    </label>
  );
}

// Keycloak's own pages send logout-sessions=on unless the person turns it off.
export function SignOutOthers({ shown, label }: { shown: boolean; label: string }) {
  return shown ? (
    <SwitchRow
      id="logout-sessions"
      name="logout-sessions"
      value="on"
      label={label}
      defaultChecked
    />
  ) : (
    <input type="hidden" id="logout-sessions" name="logout-sessions" value="on" />
  );
}

export function ActionRow({
  tile,
  title,
  detail,
  href,
  target,
  rel,
  end,
  ...button
}: {
  tile: ReactNode;
  title: ReactNode;
  detail?: ReactNode;
  href?: string;
  target?: string;
  rel?: string;
  end?: ReactNode;
} & Omit<ComponentProps<"button">, "title" | "className">) {
  const inside = (
    <>
      {tile}
      <span className="min-w-0 flex-1">
        <span className="line-clamp-2 text-[14.5px] font-medium [overflow-wrap:anywhere] text-ink">
          {title}
        </span>
        {detail && (
          <span className="line-clamp-2 text-footnote [overflow-wrap:anywhere] text-ink-muted">
            {detail}
          </span>
        )}
      </span>
      {end ?? (
        <ChevronRight
          className="size-4 shrink-0 text-ink-faint transition-transform duration-200 ease-out group-hover:translate-x-0.5"
          aria-hidden
        />
      )}
    </>
  );
  const look = cn(
    rule,
    "group flex min-h-14 w-full items-center gap-3 py-2 pr-4 pl-4 text-left transition-colors duration-200 before:left-[58px] hover:bg-lifted focus-visible:-outline-offset-2 focus-visible:first:rounded-t-[20px] focus-visible:last:rounded-b-[20px] disabled:pointer-events-none disabled:opacity-45",
  );
  return href ? (
    <a
      href={href}
      id={button.id}
      target={target}
      rel={rel}
      onClick={button.onClick as ComponentProps<"a">["onClick"]}
      className={look}
    >
      {inside}
    </a>
  ) : (
    <button type="submit" {...button} className={look}>
      {inside}
    </button>
  );
}

export function ItemRow({
  tile,
  title,
  detail,
  end,
}: {
  tile: ReactNode;
  title: ReactNode;
  detail?: ReactNode;
  end?: ReactNode;
}) {
  return (
    <div
      className={cn(rule, "flex min-h-14 items-center gap-3 py-2 pr-2.5 pl-4 before:left-[58px]")}
    >
      {tile}
      <span className="min-w-0 flex-1">
        <span className="line-clamp-2 text-[14.5px] font-medium [overflow-wrap:anywhere] text-ink">
          {title}
        </span>
        {detail && (
          <span
            className={cn(
              "text-footnote text-ink-muted",
              typeof detail === "string" ? "line-clamp-2 [overflow-wrap:anywhere]" : "block",
            )}
          >
            {detail}
          </span>
        )}
      </span>
      {end && <span className="flex shrink-0 items-center gap-1">{end}</span>}
    </div>
  );
}

export function RowAction(button: Omit<ComponentProps<"button">, "className">) {
  return (
    <button
      type="button"
      {...button}
      className="h-8 rounded-full px-3 text-footnote font-medium text-accent-strong transition-colors duration-200 hover:bg-well/70 focus-visible:-outline-offset-2 disabled:pointer-events-none disabled:opacity-45 pointer-coarse:h-11"
    />
  );
}

export function RowIcon({
  label,
  children,
  ...button
}: { label: string } & Omit<ComponentProps<"button">, "className">) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      {...button}
      className={cn(rowButton, "size-8 [&>svg]:size-[15px]")}
    >
      {children}
    </button>
  );
}
