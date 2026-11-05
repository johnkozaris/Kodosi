(function (send, listen) {
  "use strict";
  const stringify = JSON.stringify;
  const selection = getSelection.bind(window);
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

  function copied(event) {
    if (!event.isTrusted) return;
    const text = String(selection() ?? "");
    if (text) toHost({ type: "copied", text });
  }

  addEventListener("mousedown", pressed, { capture: true });
  addEventListener("copy", copied, { capture: true, passive: true });
  addEventListener("cut", copied, { capture: true, passive: true });

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
