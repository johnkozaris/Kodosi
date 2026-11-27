// Claude Code hooks bridge to the local Kodosi host. Hooks forward session
// events, relay permission answers, and submit prompts/stops without injecting
// keystrokes, passing all events through unchanged (no-op when disconnected).
//
// Do not hook `tool.call` or `tool.run`: Claude Code disables backgrounding
// and live output streaming for any tool with those hooks registered.

const VERSION = 1
// Long-poll requests normally block on the host for several seconds; three
// consecutive empty responses returning faster than QUICK_MS indicate a dead connection.
const QUICK_MS = 200
const FAILURES = 3
// The messages of the conversation that a chat gets when it links: enough to fill the window of
// items that the chat keeps, also after a restart of Kodosi or of Claude Code.
const HISTORY = 1000
const HISTORY_PART = 20
const ASKED = 64
const COMMAND_WORDS = 160
// A command that Claude Code shows in its own view of the terminal ends only when a person closes the view
// there.
const SHOWN_MS = 1500
// Claude Code ends the run of these commands while their picker stays open in the terminal.
const PICKERS = new Set(['rewind'])
// Keep batches under the host's 8 MiB limit (each UTF-16 code unit in JS
// string length can expand to up to 3 UTF-8 bytes).
const MOST_BATCH = 2 * 1024 * 1024
const pending = []
const early = []
const drained = []
const moved = []
const asked = new Map()
const loaded = Date.now().toString(36)
let socket = ''
let terminal = ''
let turn = ''
let stopping = false
let sending = false
let greeting = false
let failures = 0
let batch = 0
let asks = 0
// Track whether `bypassPermissions` was ever active: Claude Code only offers
// to skip permission checks after plan mode in sessions that had bypass on.
let mode = ''
let bypass = false
// The person's prompts in the conversation. A rewind takes the conversation back to an earlier prompt.
let turns = 0
// The command of the chat whose picker can still be open in the terminal.
let picking = 0
// The model that a person chose in Kodosi. Claude Code asks before it leaves a warm cache, in its
// terminal, where a person of the chat does not see the question.
let chosen = ''

function post($, path, body) {
  return $.http.fetch('http://kodosi' + path, {
    method: 'POST',
    socketPath: socket,
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ v: VERSION, terminal, link: loaded, ...body }),
  })
}

// Turn completion can abort an in-flight HTTP request, so failed batches retry
// with the same sequence number; the host deduplicates by batch number.
function flush($) {
  if (sending) return
  if (pending.length === 0 || !socket) {
    for (const done of drained.splice(0)) done()
    return
  }
  sending = true
  batch += 1
  send($, batch, nextBatch())
}

function nextBatch() {
  let size = 0
  let count = 0
  while (count < pending.length) {
    size += JSON.stringify(pending[count]).length
    if (count > 0 && size > MOST_BATCH) break
    count += 1
  }
  return pending.splice(0, count)
}

function send($, number, events) {
  const sent = reply => {
    if (reply && reply.ok) failures = 0
    else failures += 1
    if (failures >= FAILURES || (reply && reply.status === 410)) {
      socket = ''
      pending.length = 0
    } else if (failures > 0) {
      send($, number, events)
      return
    }
    sending = false
    flush($)
  }
  post($, '/v1/events', { batch: number, events }).then(sent, () => sent(null))
}

function tell($, kind, fields) {
  if (!socket) return
  const event = { k: kind, t: Date.now(), ...fields }
  if (greeting) {
    early.push(event)
    return
  }
  pending.push(event)
  flush($)
}

function newMode($, to, from, fields) {
  if (typeof to !== 'string' || !to) return
  mode = to
  if (to === 'bypassPermissions' || from === 'bypassPermissions') bypass = true
  tell($, 'mode', { mode: to, from: typeof from === 'string' ? from : undefined, ...fields })
}

async function modesOff($) {
  try {
    const permissions = (await $.settings.read({ key: 'permissions' })) || {}
    return {
      autoOff: (await $.settings.read({ key: 'disableAutoMode' })) === 'disable' || permissions.disableAutoMode === 'disable',
      bypassOff: permissions.disableBypassPermissionsMode === 'disable',
    }
  } catch (error) {
    return {}
  }
}

// The count of the person's prompts in the conversation as Claude Code has it now.
async function prompts($) {
  try {
    return await $.session.turns()
  } catch (error) {
    return turns
  }
}

// The slash commands of the session, in the order of Claude Code's own menu.
async function slashCommands($) {
  try {
    return (await $.command.list())
      .filter(command => !command.name.startsWith('_'))
      .map(command => ({ name: command.name, description: String(command.description || '').slice(0, COMMAND_WORDS) }))
  } catch (error) {
    return []
  }
}

// Claude Code names its module call before the reason of a refusal, as in "kodosi: $.command.run: no command
// named /x in this session".
function reason(error) {
  const words = String(error && error.message ? error.message : error).replace(/^[\w-]+: \$\.[\w.]+: /, '')
  return words.charAt(0).toUpperCase() + words.slice(1)
}

// Wait until all queued events are flushed so subsequent requests (like /v1/ask)
// refer to state the host has already received.
function settled($) {
  if (!sending && pending.length === 0) return Promise.resolve()
  return new Promise(resolve => {
    drained.push(resolve)
    flush($)
  })
}

// Bound tool inputs/outputs, which can be megabytes.
function brief(value, depth) {
  if (typeof value === 'string') return value.length > 8000 ? value.slice(0, 4000) + '\n…\n' + value.slice(-3600) : value
  if (value === null || typeof value !== 'object' || depth > 6) return value
  if (Array.isArray(value)) return value.slice(0, 100).map(each => brief(each, depth + 1))
  const out = {}
  for (const key of Object.keys(value)) out[key] = brief(value[key], depth + 1)
  return out
}

function blocks(content) {
  if (typeof content === 'string') return [{ type: 'text', text: brief(content, 0) }]
  if (!Array.isArray(content)) return []
  return content.slice(0, 100).map(block => {
    if (!block || typeof block !== 'object') return { type: 'unknown' }
    if (block.type === 'text') return { type: 'text', text: brief(block.text, 0) }
    if (block.type === 'tool_use') return { type: 'tool_use', id: block.id, name: block.name, input: brief(block.input, 0) }
    if (block.type === 'tool_result') {
      return { type: 'tool_result', tool_use_id: block.tool_use_id, is_error: block.is_error === true, content: blocks(block.content) }
    }
    return { type: String(block.type) }
  })
}

function earlier(rows) {
  const out = []
  for (const row of Array.isArray(rows) ? rows : []) {
    if (!row || (!row.text && !(row.toolUses && row.toolUses.length))) continue
    out.push({
      role: row.role,
      text: brief(row.text, 0),
      uuid: row.uuid,
      toolUses: (row.toolUses || []).map(use => ({
        tool: use.tool,
        input: brief(use.input, 0),
        text: brief(use.text, 0),
        isError: use.isError === true,
        durationMs: use.durationMs,
      })),
    })
  }
  return out
}

// Buffer live events in `early` while gathering session state so `hello` and
// `history` always arrive at the host first (host deduplicates rows by id).
async function hello($, rewound) {
  greeting = true
  const facts = { agent: 'claude', rewound: rewound || undefined }
  let rows = []
  let read = true
  try {
    facts.session = await $.session.id()
    facts.version = (await $.session.version()).version
    facts.model = await $.session.model()
    facts.cwd = await $.session.cwd()
    facts.usage = await $.session.usage()
    Object.assign(facts, await modesOff($))
    facts.commands = await slashCommands($)
    facts.settings = await offersSettings($)
    turns = await prompts($)
    rows = earlier(await $.session.messages({ last: HISTORY }))
  } catch (error) {
    read = false
  }
  greeting = false
  // A module that can no longer read its session, as at the end of Claude Code or after a reload of
  // its plugins, counts no prompts, and that is no rewind.
  if (read || !rewound) {
    if (picking) tell($, 'did', { n: picking })
    picking = 0
    facts.mode = mode || undefined
    facts.bypass = bypass
    tell($, 'hello', facts)
    for (let at = 0; at < rows.length; at += HISTORY_PART) tell($, 'history', { rows: rows.slice(at, at + HISTORY_PART) })
  }
  pending.push(...early.splice(0))
  flush($)
}

// Aborting a turn lets running commands continue in the background. Poll for
// `backgroundedByTurnAbort` records from PostToolUse and stop each via TaskStop.
// The next turn can start meanwhile, and its commands are not stopped.
async function stop($) {
  if (!turn || stopping) return
  stopping = true
  try {
    await $.turn.abort({ turnId: turn })
  } catch (error) {
    stopping = false
    return
  }
  for (let waited = 0; waited < 20; waited += 1) {
    while (moved.length > 0) {
      const command = moved.shift()
      let ok = false
      try {
        const stopped = await $.tool.call({ tool: 'TaskStop', task_id: command.task })
        ok = !(stopped && stopped.deny)
      } catch (error) {
        ok = false
      }
      tell($, 'halted', { id: command.call, ok })
    }
    await $.clock.sleep(50)
  }
  stopping = false
  moved.length = 0
}

// Claude Code refuses a module that reads a call of $ as a value, so only a call can tell whether
// this Claude Code has its settings for a module.
async function offersSettings($) {
  try {
    await $.config.list()
    return true
  } catch (error) {
    return false
  }
}

// The settings of Claude Code's /config view, which Claude Code checks and saves itself.
async function settings($) {
  let rows = []
  try {
    rows = await $.config.list()
  } catch (error) {
    rows = []
  }
  tell($, 'settings', { rows: Array.isArray(rows) ? rows : [] })
}

async function set($, command) {
  const key = String(command.key)
  try {
    const result = await $.config.set({ key, value: command.value })
    if (result && result.deny !== undefined) {
      tell($, 'set', { key, deny: String(result.deny) })
      return
    }
    tell($, 'set', { key, value: result && result.value !== undefined ? result.value : command.value })
  } catch (error) {
    tell($, 'set', { key, deny: reason(error) })
    return
  }
  settings($)
}

// A message goes in as the person's own words, and one that starts with a slash runs that command, as in the
// terminal. Claude Code starts its turn when the session is idle. A change of the model, the effort or fast
// mode from Kodosi's own controls runs as its command.
function run($, command) {
  if (command.do === 'stop') stop($)
  if (command.do === 'settings') settings($)
  if (command.do === 'set') set($, command)
  if (command.do !== 'prompt' && command.do !== 'change') return
  if (picking) tell($, 'did', { n: picking })
  picking = 0
  const text = String(command.text)
  const slash = /^\s*\/(\S+)\s*([\s\S]*)$/.exec(text)
  const model = slash && slash[1] === 'model' ? slash[2].trim().toLowerCase() : ''
  if (model) chosen = model
  const sent = slash ? $.command.run({ command: slash[1], args: slash[2] }) : $.prompt.submit({ text, asUser: true })
  let ended = false
  sent.then(
    result => {
      const drop = result && result.drop ? String(result.drop) : undefined
      if (slash && PICKERS.has(slash[1]) && !drop) {
        picking = command.n
        tell($, 'waits', { n: command.n, picker: true })
      } else tell($, 'did', { n: command.n, drop })
    },
    error => tell($, 'did', { n: command.n, error: reason(error) }),
  ).finally(() => {
    ended = true
    if (model && chosen === model) chosen = ''
  })
  if (slash) {
    $.clock.sleep(SHOWN_MS).then(() => {
      if (!ended && !turn) tell($, 'waits', { n: command.n })
    })
  }
}

async function listen($) {
  let quick = 0
  while (socket && quick < FAILURES) {
    const began = Date.now()
    let commands = []
    try {
      const reply = await post($, '/v1/next', {})
      if (reply.ok && reply.text) commands = JSON.parse(reply.text).commands
    } catch (error) {
      commands = []
    }
    if (!Array.isArray(commands)) commands = []
    for (const command of commands) run($, command)
    quick = commands.length === 0 && Date.now() - began < QUICK_MS ? quick + 1 : 0
  }
}

async function ask($, body) {
  try {
    const reply = await post($, '/v1/ask', body)
    if (!reply.ok) return { closed: true }
    return reply.text ? JSON.parse(reply.text) : null
  } catch (error) {
    return { closed: true }
  }
}

export function register(on) {
  on('session.start', async ($, e, next) => {
    const started = await next(e)
    const link = e.isInteractive ? (await $.env.get('KODOSI_AGENT_LINK')) || '' : ''
    const id = link ? (await $.env.get('KODOSI_SESSION_ID')) || '' : ''
    if (!id) return started
    terminal = id
    socket = link
    failures = 0
    await hello($)
    listen($)
    return started
  })

  // /clear and /resume switch conversations in-place without firing session.start.
  on('session.end', async ($, e, next) => {
    tell($, 'end', { reason: e.reason })
    const ended = await next(e)
    if (socket && (e.reason === 'clear' || e.reason === 'resume')) hello($)
    return ended
  })

  on('session.stop', async ($, e, next) => {
    tell($, 'stop', {})
    return next(e)
  })

  on('session.measure', async ($, e, next) => {
    tell($, 'measure', { context: e.context, limits: e.rateLimits, cost: e.cost })
    return next(e)
  })

  on('session.append', async ($, e, next) => {
    const door = e.door
    const message = e.message || {}
    if (door === 'prompt' || door === 'command' || door === 'response' || door === 'tool-result') {
      tell($, 'row', {
        door,
        origin: e.origin,
        uuid: e.uuid,
        agent: e.agentId,
        content: blocks(message.content),
      })
    } else if (message.type === 'system' && message.name === 'away_summary' && !e.agentId) {
      tell($, 'recap', { uuid: e.uuid, content: blocks(message.content) })
    }
    return next(e)
  })

  on('turn.start', async ($, e, next) => {
    if (!e.agentId) {
      turn = e.turnId
      turns = await prompts($)
    }
    tell($, 'turn', { agent: e.agentId })
    return next(e)
  })

  // A rewind puts the prompt it went back to in the prompt box again.
  on('prompt.change', async ($, e, next) => {
    const changed = await next(e)
    if (socket && !greeting && (await prompts($)) < turns) hello($, true)
    return changed
  })

  // Subagent activity is rendered from session.append rows; only stream the main thread.
  on('turn.step', async function* ($, e, next) {
    if (!socket || e.agentId) return yield* next(e)
    tell($, 'step', { model: e.model, effort: e.effort })
    const stream = next(e)
    let thought = -1
    let item = await stream.next()
    while (!item.done) {
      const piece = item.value
      if (piece.kind === 'text') tell($, 'text', { block: piece.index, text: piece.text })
      else if (piece.kind === 'tool') tell($, 'tool', { id: piece.id, name: piece.name })
      else if (piece.kind === 'thinking' && piece.index !== thought) {
        thought = piece.index
        tell($, 'think', {})
      }
      yield piece
      item = await stream.next()
    }
    tell($, 'stepEnd', {})
    return item.value
  })

  on('turn.complete', async ($, e, next) => {
    if (!e.agentId && turn === e.turnId) turn = ''
    tell($, 'turnEnd', { agent: e.agentId, ms: e.durationMs, reason: e.reason })
    return next(e)
  })

  // PermissionRequest omits tool_use_id, so cache it from the preceding tool.check.
  on('tool.check', async ($, e, next) => {
    const decided = await next(e)
    if (socket && decided && decided.decision === 'ask' && e.tool_use_id) {
      if (asked.size >= ASKED) asked.delete(asked.keys().next().value)
      asked.set(e.tool + JSON.stringify(e.input), e.tool_use_id)
    }
    return decided
  })

  // Claude Code shows the same prompt in the terminal concurrently; either side
  // can answer first, and the host closes the /v1/ask wait once the question is gone.
  on('classic.PermissionRequest', async ($, e, next) => {
    if (!socket) return next(e)
    asks += 1
    const id = loaded + '-' + asks
    const key = e.tool_name + JSON.stringify(e.tool_input)
    const call = asked.get(key)
    asked.delete(key)
    tell($, 'ask', {
      ask: id,
      call,
      tool: e.tool_name,
      input: brief(e.tool_input, 0),
      suggestions: e.permission_suggestions,
      mode: e.permission_mode,
      agent: e.agent_id,
    })
    await settled($)
    let answer = null
    let quick = 0
    while (socket && !answer && !next.signal.aborted && quick < FAILURES) {
      const began = Date.now()
      answer = await ask($, { ask: id })
      quick = !answer && Date.now() - began < QUICK_MS ? quick + 1 : 0
    }
    if (answer && answer.decision) return { decision: answer.decision }
    return next(e)
  })

  // `telemetry.log` is the only hook that reports permission decisions and mode
  // changes; only the tool call, the decision and the modes are forwarded, nothing
  // about the person or the account.
  on('telemetry.log', { to: 'collector' }, async ($, e, next) => {
    if (e.event === 'tool_decision' && e.attributes) {
      tell($, 'decided', {
        id: e.attributes.tool_use_id,
        decision: e.attributes.decision,
        mode: e.attributes.permission_mode,
      })
    } else if (e.event === 'permission_mode_changed' && e.attributes) {
      newMode($, e.attributes.to_mode, e.attributes.from_mode)
    }
    return next(e)
  })

  on('classic.PostToolUse', async ($, e, next) => {
    tell($, 'done', { id: e.tool_use_id, ms: e.duration_ms, result: brief(e.tool_response, 0) })
    const record = e.tool_response
    if (stopping && record && record.backgroundedByTurnAbort && typeof record.backgroundTaskId === 'string') {
      moved.push({ call: e.tool_use_id, task: record.backgroundTaskId })
    }
    return next(e)
  })

  on('agent.spawn', async ($, e, next) => {
    const spawned = await next(e)
    tell($, 'agent', { call: e.tool_use_id, id: spawned ? spawned.agentId : undefined, description: e.description })
    return spawned
  })

  on('classic.UserPromptSubmit', async ($, e, next) => {
    newMode($, e.permission_mode)
    return next(e)
  })

  on('classic.Stop', async ($, e, next) => {
    newMode($, e.permission_mode, undefined, {
      background: (e.background_tasks || []).filter(task => task.type === 'shell' && task.status === 'running').length,
    })
    return next(e)
  })

  on('classic.PostModelSwitch', async ($, e, next) => {
    tell($, 'model', { model: e.to_model })
    return next(e)
  })

  // A helper compacts its own conversation too, and the chat shows the main one.
  on('classic.PreCompact', async ($, e, next) => {
    if (!e.agent_id) tell($, 'compact', { phase: 'start' })
    return next(e)
  })

  on('classic.PostCompact', async ($, e, next) => {
    if (!e.agent_id) tell($, 'compact', { phase: 'end' })
    return next(e)
  })

  on('classic.PreModelSwitch', async ($, e, next) => {
    const decided = await next(e)
    const requested = String(e.requested_model || 'default').toLowerCase()
    if (!chosen || e.source !== 'command' || requested !== chosen) return decided
    return decided && decided.permissionDecision ? decided : { ...decided, permissionDecision: 'allow' }
  })

  on('classic.StopFailure', async ($, e, next) => {
    tell($, 'failure', { error: brief(e.error, 0), details: brief(e.error_details, 0) })
    return next(e)
  })

  on('classic.CwdChanged', async ($, e, next) => {
    tell($, 'cwd', { cwd: e.new_cwd })
    return next(e)
  })
}
