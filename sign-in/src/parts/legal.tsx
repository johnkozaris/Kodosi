import { Fragment } from "react";

import { QuietLink } from "./text";

/** An address that the server gave, when a link can open it. */
function web(address: string | undefined): string | undefined {
  return address && /^https?:\/\//i.test(address) ? address : undefined;
}

/**
 * The terms and the privacy page of the service, at the foot of each page. The Keycloak server
 * gives the addresses, so a server without them shows no links. They open in a new tab, so a form
 * that the person fills stays as it is.
 */
export function Legal({ links }: { links: { label: string; href: string | undefined }[] }) {
  const shown = links.flatMap(({ label, href }) => {
    const to = web(href);
    return to ? [{ label, href: to }] : [];
  });
  if (shown.length === 0) return null;
  return (
    <p className="text-footnote text-ink-faint">
      {shown.map(({ label, href }, i) => (
        <Fragment key={href}>
          {i > 0 && (
            <span aria-hidden className="mx-1.5">
              ·
            </span>
          )}
          <QuietLink
            href={href}
            target="_blank"
            rel="noreferrer"
            className="font-normal text-ink-muted hover:text-ink"
          >
            {label}
          </QuietLink>
        </Fragment>
      ))}
    </p>
  );
}
