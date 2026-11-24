// Kodosi's link to Claude Code: a hooks module that tells the Kodosi host on this computer what this session
// does, gives Claude Code the person's answer to a question, and runs what the person sends from Kodosi.
// It changes nothing else in the session: each hook passes its event on as it came, and when the host is away
// each hook does nothing.
//
// It has no hook on tool.call or tool.run: Claude Code turns off the move of a command to the background and
// its live output for a tool that has one.
//
// It presses no key. A stop from Kodosi ends the turn with Claude Code's own call. Claude Code then lets a
// command of the turn run on in the background, so the module also stops that command.

const VERSION = 1
// The host holds a request open for many seconds. An empty answer that comes faster did not wait there, and
// three of them in a row end the wait.
const QUICK_MS = 200
const FAILURES = 3
const HISTORY = 120
const HISTORY_PART = 20
const ASKED = 64
const COMMAND_WORDS = 160
// A command that Claude Code shows in its own view of the terminal ends only when a person closes the view
// there.
const SHOWN_MS = 1500
// Claude Code ends the run of these commands while their picker stays open in the terminal.
const PICKERS = new Set(['rewind'])
// The host takes at most 8 MiB in a request, and a character of the JSON takes up to three bytes there.
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
// The permission mode of the session. Claude Code offers to go on without permission checks after a plan
// only in a session that started without them.
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

// The end of a turn can break a request that is on its way, so a batch that got no answer goes again
// under its number, and the host takes each number once.
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

// The settings that take permission modes out of the session.
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

// The host has each event that came before, so a request that follows finds what it names.
function settled($) {
  if (!sending && pending.length === 0) return Promise.resolve()
  return new Promise(resolve => {
    drained.push(resolve)
    flush($)
  })
}

// A tool can take or return megabytes. The chat shows the start and the end.
function brief(value, depth) {
  if (typeof value === 'string') return value.length > 8000 ? value.slice(0, 4000) + '\n…\n' + value.slice(-3600) : value
  if (value === null || typeof value !== 'object' || depth > 6) return value
  if (Array.isArray(value)) return value.slice(0, 100).map(each => brief(each, depth + 1))
  const out = {}
  for (const key of Object.keys(value)) out[key] = brief(value[key], depth + 1)
  return out
}

// The text and the tool calls of a row. Thinking and pictures go as their kind alone.
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

// The host follows the module from its hello, so what happens while the module reads the session waits
// until the hello and the history are out. The host knows each row of the history by its id.
async function hello($, rewound) {
  if (picking) tell($, 'did', { n: picking })
  picking = 0
  greeting = true
  const facts = { agent: 'claude', rewound: rewound || undefined }
  let rows = []
  try {
    facts.session = await $.session.id()
    facts.version = (await $.session.version()).version
    facts.model = await $.session.model()
    facts.cwd = await $.session.cwd()
    facts.usage = await $.session.usage()
    Object.assign(facts, await modesOff($))
    facts.commands = await slashCommands($)
    turns = await prompts($)
    rows = earlier(await $.session.messages({ last: HISTORY }))
  } catch (error) {
    rows = []
  }
  facts.mode = mode || undefined
  facts.bypass = bypass
  greeting = false
  tell($, 'hello', facts)
  for (let at = 0; at < rows.length; at += HISTORY_PART) tell($, 'history', { rows: rows.slice(at, at + HISTORY_PART) })
  pending.push(...early.splice(0))
  flush($)
}

// Claude Code's own end of a turn lets a command of the turn run on in the background. Its record comes
// with the end, and the module then stops the command with Claude Code's own tool. The next turn can start
// in this time, and its commands are not of the stop.
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

// A message goes in as the person's own words, and one that starts with a slash runs that command, as in the
// terminal. Claude Code starts its turn when the session is idle. A change of the model, the effort or fast
// mode from Kodosi's own controls runs as its command.
function run($, command) {
  if (command.do === 'stop') stop($)
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
    error => tell($, 'did', { n: command.n, error: String(error) }),
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

// What the person sends from Kodosi. The loop ends with the session, or when the host is away.
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

  // After /clear and /resume the process goes on with another conversation, and Claude Code starts no session.
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

  // The chat shows a helper's steps from its rows, so only the main conversation reports as it writes.
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

  // The question of Claude Code does not name its tool call. The check just before it does.
  on('tool.check', async ($, e, next) => {
    const decided = await next(e)
    if (socket && decided && decided.decision === 'ask' && e.tool_use_id) {
      if (asked.size >= ASKED) asked.delete(asked.keys().next().value)
      asked.set(e.tool + JSON.stringify(e.input), e.tool_use_id)
    }
    return decided
  })

  // Claude Code shows its own question at the same time, so the person can answer in the terminal too.
  // The host ends this wait when the question is gone.
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

  // Claude Code reports the moment and the source of each permission decision, and each change of the
  // permission mode, only in these records. The hook reads the tool call, the decision and the modes from
  // them, and nothing about the person or the account.
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
      background: (e.background_tasks || []).filter(task => task.status === 'running').length,
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
