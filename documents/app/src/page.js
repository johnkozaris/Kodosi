// Shows the window's file once the view bundle loads, and carries its pipe on Tauri channels.
(() => {
  const dark = matchMedia("(prefers-color-scheme: dark)");
  const look = () => ({ scheme: dark.matches ? "dark" : "light" });

  async function show(views) {
    const { invoke, Channel } = window.__TAURI__.core;
    let receive = () => {};
    const frames = new Channel();
    frames.onmessage = (frame) => receive(new Uint8Array(frame));
    const kind = await invoke("open", { frames });
    const element = document.body.appendChild(document.createElement("div"));
    element.style.height = "100vh";
    const view = views.mount(element, { kind, theme: look() });
    dark.addEventListener("change", () => view.restyle(look()));
    let sent = Promise.resolve();
    view.attach({
      send(frame) {
        sent = sent.then(() => invoke("send", frame)).catch(console.error);
      },
      onMessage(handler) {
        receive = handler;
      },
    });
  }

  Object.defineProperty(window, "documentView", {
    configurable: true,
    set(views) {
      Object.defineProperty(window, "documentView", { value: views, writable: true });
      show(views).catch((problem) => {
        document.body.textContent = String(problem);
      });
    },
  });
})();
