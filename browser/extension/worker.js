const OFFSCREEN = "offscreen.html";
const REPORT_DELAY = 50;

let reportTimer = null;

async function ensureOffscreen() {
  if (await chrome.offscreen.hasDocument()) return;
  await chrome.offscreen.createDocument({
    url: OFFSCREEN,
    reasons: ["USER_MEDIA"],
    justification: "Encodes the watched tab for the people in its terminal.",
  });
}

async function report() {
  const [windows, tabs, targets] = await Promise.all([
    chrome.windows.getAll(),
    chrome.tabs.query({}),
    chrome.debugger.getTargets(),
  ]);
  const targetOf = new Map(targets.filter((target) => target.tabId).map((target) => [target.tabId, target.id]));
  await send({
    type: "tabs",
    windows: windows.map((window) => ({ id: window.id, popup: window.type === "popup" })),
    tabs: tabs.map((tab) => ({
      id: tab.id,
      window: tab.windowId,
      opener: tab.openerTabId ?? null,
      target: targetOf.get(tab.id) ?? null,
      title: tab.title ?? "",
      address: tab.url || tab.pendingUrl || "",
      active: tab.active,
      loading: tab.status === "loading",
    })),
  });
}

function reportSoon() {
  clearTimeout(reportTimer);
  reportTimer = setTimeout(() => report().catch(() => {}), REPORT_DELAY);
}

async function send(message) {
  await ensureOffscreen();
  await chrome.runtime.sendMessage({ to: "offscreen", message });
}

const commands = {
  async activate({ tab }) {
    await chrome.tabs.update(tab, { active: true });
  },
  async newTab({ window }) {
    await chrome.tabs.create({ windowId: window, url: "about:blank", active: true });
  },
  async closeTab({ tab }) {
    await chrome.tabs.remove(tab);
  },
  async closeWindow({ window }) {
    await chrome.windows.remove(window);
  },
  async pick({ tab, frame, menu, choice }) {
    await chrome.tabs.sendMessage(tab, { menu, choice }, { frameId: frame });
  },
  async report() {
    await report();
  },
  async streamId({ tab }) {
    return chrome.tabCapture.getMediaStreamId({ targetTabId: tab });
  },
};

chrome.runtime.onMessage.addListener((request, sender, answer) => {
  if (request.to === "worker") {
    const command = commands[request.command.type];
    command?.(request.command).then(answer, (error) => answer({ error: String(error) }));
    return true;
  }
  if (request.to === "host" && sender.tab) {
    send({ ...request.message, tab: sender.tab.id, frame: sender.frameId }).catch(() => {});
  }
  return false;
});

for (const event of [
  chrome.tabs.onActivated,
  chrome.tabs.onCreated,
  chrome.tabs.onRemoved,
  chrome.tabs.onUpdated,
  chrome.tabs.onAttached,
  chrome.windows.onCreated,
  chrome.windows.onRemoved,
]) {
  event.addListener(reportSoon);
}

chrome.runtime.onInstalled.addListener(() => ensureOffscreen());
chrome.runtime.onStartup.addListener(() => ensureOffscreen());
