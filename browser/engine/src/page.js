(function (send, listen) {
  "use strict";
  const stringify = JSON.stringify;
  const selection = getSelection.bind(window);
  const on = addEventListener.bind(window);
  const off = removeEventListener.bind(window);
  const soon = setTimeout.bind(window);
  const command = document.execCommand.bind(document);
  const can = document.queryCommandEnabled.bind(document);
  let waiting = null;
  let nextMenu = 1;

  function toHost(message) {
    send(stringify(message));
  }

  function pickFrom(select) {
    const menu = nextMenu++;
    waiting = { menu, select };
    toHost({
      type: "menu",
      menu,
      items: Array.from(select.options, (option) => ({
        label: option.label || option.text,
        enabled: !option.disabled,
        chosen: option.selected,
      })),
    });
  }

  function pressed(event) {
    if (!event.isTrusted) return;
    const select = event.target instanceof Element ? event.target.closest("select") : null;
    if (!select || select.multiple || select.size > 1 || select.disabled) return;
    event.preventDefault();
    select.focus();
    pickFrom(select);
  }

  // Forward copied text to the host instead of writing to the host machine's clipboard. We
  // attach a bubble listener to read text after page handlers run (which may override
  // clipboardData) and cancel the native copy; the setTimeout fallback still forwards text if
  // a page handler calls stopPropagation(), though the browser then also copies natively.
  function copied(event) {
    if (!event.isTrusted) return;
    const selected = String(selection() ?? "");
    // Native cut is a no-op outside editable elements unless a page handler intercepts it.
    const cuts = event.type === "cut" && can("cut");
    let told = false;
    const words = () => {
      if (event.defaultPrevented) return event.clipboardData?.getData("text/plain") ?? "";
      return event.type === "copy" || cuts ? selected : "";
    };
    const last = (leaving) => {
      if (leaving !== event || told) return;
      told = true;
      off(event.type, last);
      const text = words();
      if (!text) return;
      const own = event.defaultPrevented;
      toHost({ type: "copied", text });
      event.clipboardData?.clearData();
      event.preventDefault();
      // Cancelling cut prevents native deletion, and execCommand cannot run reentrantly inside
      // the cut event, so defer the delete.
      if (cuts && !own) soon(() => command("delete"), 0);
    };
    on(event.type, last);
    soon(() => {
      if (told) return;
      told = true;
      off(event.type, last);
      const text = words();
      if (text) toHost({ type: "copied", text });
    }, 0);
  }

  // Intercept Async Clipboard API writes, which do not fire DOM copy events.
  function ownClipboard() {
    if (!navigator.clipboard) return;
    const clipboard = Object.getPrototypeOf(navigator.clipboard);
    const write = clipboard.write;
    const refused = () => Promise.reject(new DOMException("Write permission denied.", "NotAllowedError"));
    const allowed = () =>
      navigator.userActivation?.isActive !== false && document.featurePolicy?.allowsFeature("clipboard-write") !== false;
    const give = (name, value) => Object.defineProperty(clipboard, name, { value, writable: true, configurable: true });
    give("writeText", function (text) {
      if (!allowed()) return refused();
      toHost({ type: "copied", text: String(text) });
      return Promise.resolve();
    });
    give("write", async function (items) {
      const plain = Array.from(items ?? []).find((item) => item?.types?.includes("text/plain"));
      if (!plain) return write.call(this, items);
      if (!allowed()) return refused();
      const text = await (await plain.getType("text/plain")).text();
      toHost({ type: "copied", text });
    });
  }

  on("mousedown", pressed, { capture: true });
  on("copy", copied, { capture: true, passive: true });
  on("cut", copied, { capture: true, passive: true });
  ownClipboard();

  listen((menu, choice) => {
    if (!waiting || waiting.menu !== menu) return;
    const { select } = waiting;
    waiting = null;
    if (choice < 0 || choice === select.selectedIndex) return;
    select.selectedIndex = choice;
    select.dispatchEvent(new Event("input", { bubbles: true }));
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
});
