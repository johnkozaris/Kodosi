import type { ComponentProps, ReactNode } from "react";

import { cn } from "../parts/cn";
import { carried } from "./carry";

export function Page({
  wide,
  banner,
  foot,
  children,
  ...events
}: {
  wide?: boolean | undefined;
  banner?: boolean | undefined;
  foot: ReactNode;
  children: ReactNode;
} & Pick<ComponentProps<"div">, "onInput" | "onSubmit">) {
  return (
    <div className="flex min-h-dvh flex-col" {...events}>
      <main
        className={cn(
          "mx-auto flex w-full flex-col items-center px-4 pt-(--stage-top) pb-10",
          wide ? "max-w-[520px]" : "max-w-[432px]",
          !carried() && "rise",
        )}
      >
        <div
          aria-hidden
          className={banner ? "h-[calc(var(--banner-size)*1.5)]" : "h-[calc(var(--logo-size)*1.5)]"}
        />
        {children}
      </main>
      <footer className="mt-auto flex flex-col items-center gap-3 px-4 pb-[max(1.5rem,env(safe-area-inset-bottom))]">
        {foot}
      </footer>
    </div>
  );
}
