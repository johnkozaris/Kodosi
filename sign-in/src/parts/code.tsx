import { Fragment, useEffect, useRef, useState } from "react";

import { cn } from "./cn";
import { NOTE_ID } from "./text";

const KINDS = {
  /** The code of an authenticator app. */
  digits: { strip: /\D/g, pattern: "[0-9]", mode: "numeric", complete: "one-time-code" },
  /** The code that the Kodosi app shows: letters and digits. */
  letters: { strip: /[^A-Za-z0-9]/g, pattern: "[A-Za-z0-9]", mode: "text", complete: "off" },
} as const;

/** The small line between the two halves of a code. */
function Dash() {
  return <span aria-hidden className="h-0.5 w-2 shrink-0 rounded-full bg-ink-faint/60" />;
}

/**
 * A code that the person types. One real field takes the typing, the paste and the code that a
 * phone offers, and the cells show it. The cell that takes the next letter holds the place of
 * the page's cursor. The last letter sends the form.
 */
export function CodeCells({
  name,
  id,
  length,
  kind = "digits",
  half,
  label,
  autoFocus,
  invalid,
  send = true,
  given,
}: {
  name: string;
  id?: string;
  length: number;
  kind?: keyof typeof KINDS;
  /** The code has two halves with a line between them, and the field sends it with that line. */
  half?: boolean;
  label: string;
  autoFocus?: boolean;
  /** Keycloak refused the code. */
  invalid?: boolean;
  /** The last letter sends the form. A step with more fields sends with its arrow. */
  send?: boolean;
  /** The code came with the page: the cells show it letter by letter, and the form goes. */
  given?: string | null;
}) {
  const { strip, pattern, mode, complete } = KINDS[kind];
  const [value, setValue] = useState(() =>
    (given ?? "").replace(strip, "").slice(0, length).toUpperCase(),
  );
  const [focused, setFocused] = useState(false);
  const field = useRef<HTMLInputElement>(null);
  const sent = useRef(false);

  useEffect(() => {
    if (!given || !send || value.length !== length || sent.current) return;
    sent.current = true;
    // The person sees the code come into the cells before the page goes on.
    const timer = window.setTimeout(() => field.current?.form?.requestSubmit(), 760);
    return () => {
      clearTimeout(timer);
      sent.current = false;
    };
    // The code that came with the page goes one time.
    // oxlint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const at = Math.min(value.length, length - 1);
  const middle = length / 2;
  const written =
    half && value.length > middle ? `${value.slice(0, middle)}-${value.slice(middle)}` : value;
  return (
    <div data-cursor-host className="relative px-4 py-4">
      <div aria-hidden className="flex items-center justify-center gap-1.5 sm:gap-2">
        {Array.from({ length }, (_, i) => {
          const letter = value[i];
          return (
            <Fragment key={i}>
              {half && i === middle && <Dash />}
              <span
                data-at={focused && i === at ? "" : undefined}
                className="cell well relative grid h-[52px] w-11 min-w-0 shrink place-items-center rounded-md font-mono text-[22px] font-semibold text-ink"
              >
                {letter && (
                  <span
                    key={letter + i}
                    className="pop"
                    style={given ? { animationDelay: `${i * 45}ms` } : undefined}
                  >
                    {letter}
                  </span>
                )}
                {focused && i === at && (
                  <span data-cursor-slot className="cell-slot absolute h-[27px] w-[13px]" />
                )}
              </span>
            </Fragment>
          );
        })}
      </div>
      <input
        ref={field}
        id={id}
        name={name}
        aria-label={label}
        aria-invalid={invalid || undefined}
        aria-describedby={invalid ? NOTE_ID : undefined}
        autoFocus={autoFocus}
        inputMode={mode}
        autoComplete={complete}
        autoCapitalize="characters"
        spellCheck={false}
        dir="ltr"
        required
        pattern={half ? `${pattern}{${middle}}-${pattern}{${middle}}` : `${pattern}{${length}}`}
        value={written}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onChange={(event) => {
          const next = event.target.value.replace(strip, "").slice(0, length).toUpperCase();
          setValue(next);
          if (send && next.length === length && !sent.current) {
            sent.current = true;
            const form = event.target.form;
            // The cells show the last letter before the page goes.
            window.setTimeout(() => form?.requestSubmit(), 180);
          }
          if (next.length < length) sent.current = false;
        }}
        className="code-input absolute inset-0 size-full cursor-text opacity-0"
      />
    </div>
  );
}

/**
 * A code that the page shows: the tiles of the app's sign-in window, so the person sees the same
 * object in the app and in the browser. A screen reader says the code letter by letter.
 */
export function CodeTiles({
  code,
  label,
  small,
  className,
}: {
  code: string;
  /** What the code is, for a screen reader. */
  label: string;
  small?: boolean;
  className?: string;
}) {
  const letters = [...code];
  return (
    <span
      className={cn(
        "inline-flex items-center",
        small ? "gap-[3px]" : "gap-[5px] sm:gap-1.5",
        className,
      )}
    >
      <span className="sr-only">
        {label}: {letters.filter((one) => one !== "-").join(" ")}
      </span>
      {letters.map((letter, i) =>
        letter === "-" || letter === " " ? (
          // oxlint-disable-next-line react/no-array-index-key -- the place of a letter in a code is what it is
          <span key={i} aria-hidden className={cn("flex justify-center", small ? "w-2" : "w-3")}>
            <Dash />
          </span>
        ) : (
          <span
            // oxlint-disable-next-line react/no-array-index-key -- the place of a letter in a code is what it is
            key={i}
            aria-hidden
            style={{ animationDelay: `${i * 30}ms` }}
            className={cn(
              "sheet rise grid place-items-center bg-lifted font-mono font-semibold text-ink",
              small
                ? "h-7 w-[21px] rounded-xs text-[14px]"
                : "h-[46px] w-[34px] rounded-sm text-[22px] sm:w-9",
            )}
          >
            {letter}
          </span>
        ),
      )}
    </span>
  );
}
