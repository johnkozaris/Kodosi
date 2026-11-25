// Copilot CLI extension bridging session events, prompts, and approval dialogs
// to the local Kodosi host without injecting keystrokes. No-ops outside Kodosi.

import { joinSession } from '@microsoft/copilot-sdk/extension'
import http from 'node:http'

const VERSION = 1
// Long-poll requests normally block on the host for several seconds; three
// consecutive empty responses returning faster than QUICK_MS indicate a dead connection.
const QUICK_MS = 200
const FAILURES = 3
const HISTORY = 400
const HISTORY_PART = 40
// Keep batches under the host's 8 MiB limit (each UTF-16 code unit in JS
// string length can expand to up to 3 UTF-8 bytes).
const MOST_BATCH = 2 * 1024 * 1024
// Allowlist of events and fields forwarded to Kodosi, skipping bulky unused
// payloads like system prompts, partial tool args, and encrypted reasoning.
const READ = {
  'user.message': ['content'],
  'assistant.message_start': ['messageId'],
  'assistant.message_delta': ['messageId', 'deltaContent'],
  'assistant.message': ['messageId', 'content'],
  'assistant.turn_start': [],
  'session.idle': [],
  'session.error': ['errorType', 'message'],
  abort: [],
  'tool.execution_start': ['toolCallId', 'toolName', 'arguments', 'toolTitle'],
  'tool.shell_output': ['toolCallId', 'text'],
  'tool.execution_complete': ['toolCallId', 'success', 'result'],
  'permission.completed': ['toolCallId', 'result'],
  'elicitation.requested': ['requestId', 'toolCallId'],
  'elicitation.completed': ['requestId', 'action', 'content'],
  'exit_plan_mode.completed': ['requestId', 'approved'],
  'subagent.started': ['toolCallId', 'agentName', 'agentDisplayName'],
  'subagent.completed': [],
  'subagent.failed': [],
  'session.usage_info': ['tokenLimit', 'currentTokens'],
  'session.model_change': ['newModel', 'reasoningEffort'],
  'session.mode_changed': ['newMode'],
}

// On Windows, Node's `http.request` connects to a named pipe (`KODOSI_AGENT_PIPE`)
// rather than the Unix socket path in `KODOSI_AGENT_LINK`.
let socket = process.env.KODOSI_AGENT_PIPE || process.env.KODOSI_AGENT_LINK || ''
const terminal = process.env.KODOSI_SESSION_ID || ''
const loaded = Date.now().toString(36)
const pending = []
const drained = []
let sending = false
let failures = 0
let batch = 0
let asks = 0
let session = null
let reading = null
let readAgain = false

function post(path, body) {
  return new Promise(resolve => {
    const data = JSON.stringify({ v: VERSION, terminal, link: loaded, ...body })
    const request = http.request(
      { socketPath: socket, path, method: 'POST', headers: { 'content-type': 'application/json' } },
      response => {
        const parts = []
        response.on('data', part => parts.push(part))
        response.on('end', () => {
          const status = response.statusCode || 0
          resolve({ ok: status >= 200 && status < 300, status, text: Buffer.concat(parts).toString('utf8') })
        })
        response.on('error', () => resolve(null))
      },
    )
    request.on('error', () => resolve(null))
    request.end(data)
  })
}

// Turn completion can abort an in-flight HTTP request, so failed batches retry
// with the same sequence number; the host deduplicates by batch number.
function flush() {
  if (sending) return
  if (pending.length === 0 || !socket) {
    for (const done of drained.splice(0)) done()
    return
  }
  sending = true
  batch += 1
  send(batch, nextBatch())
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

function send(number, events) {
  post('/v1/events', { batch: number, events }).then(reply => {
    if (reply && reply.ok) failures = 0
    else failures += 1
    if (failures >= FAILURES || (reply && reply.status === 410)) {
      socket = ''
      pending.length = 0
    } else if (failures > 0) {
      send(number, events)
      return
    }
    sending = false
    flush()
  })
}

function tell(kind, fields) {
  if (!socket) return
  pending.push({ k: kind, t: Date.now(), ...fields })
  flush()
}

// Wait until all queued events are flushed so subsequent requests (like /v1/ask)
// refer to state the host has already received.
function settled() {
  if (!sending && pending.length === 0) return Promise.resolve()
  return new Promise(resolve => {
    drained.push(resolve)
    flush()
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

function shaped(event) {
  const fields = event && READ[event.type]
  if (!fields) return null
  const data = {}
  for (const field of fields) if (event.data && event.data[field] !== undefined) data[field] = brief(event.data[field], 0)
  return { type: event.type, data, id: event.id, agent: event.agentId, at: Date.parse(event.timestamp) || undefined }
}

// `session.todos_changed` carries no payload, so fetch the todo list from
// Copilot's session DB and coalesce overlapping refreshes.
function readTasks() {
  if (reading) {
    readAgain = true
    return reading
  }
  reading = session.rpc.plan
    .readSqlTodosWithDependencies()
    .then(
      todos => {
        const rows = todos && Array.isArray(todos.rows) ? todos.rows : []
        tell('tasks', { rows: rows.slice(0, 64).map(row => ({ title: row.title, status: row.status })) })
      },
      () => {},
    )
    .then(() => {
      reading = null
      if (readAgain) {
        readAgain = false
        return readTasks()
      }
    })
  return reading
}

// Send `hello` and `history` first, then flush live events buffered in `early`
// that were not already included in history.
async function hello(early) {
  const facts = { agent: 'copilot', version: process.env.COPILOT_CLI_BINARY_VERSION, cwd: process.cwd() }
  try {
    const current = await session.rpc.model.getCurrent()
    facts.model = current && current.modelId
  } catch (error) {
    facts.model = undefined
  }
  try {
    const mode = await session.rpc.mode.get()
    facts.mode = typeof mode === 'string' ? mode : mode && mode.mode
  } catch (error) {
    facts.mode = undefined
  }
  let earlier = []
  try {
    earlier = (await session.getEvents()).map(shaped).filter(Boolean).slice(-HISTORY)
  } catch (error) {
    earlier = []
  }
  tell('hello', facts)
  for (let at = 0; at < earlier.length; at += HISTORY_PART) {
    tell('history', { events: earlier.slice(at, at + HISTORY_PART), more: at + HISTORY_PART < earlier.length })
  }
  const known = new Set(earlier.map(event => event.id))
  for (const event of early.splice(0)) if (!known.has(event.id)) tell('event', event)
  await readTasks()
}

async function run(command) {
  if (command.do === 'stop') {
    try {
      await session.abort()
    } catch (error) {
      tell('did', { stop: true, error: String(error) })
    }
    return
  }
  if (command.do !== 'prompt') return
  try {
    if (command.mode) await session.rpc.mode.set({ mode: String(command.mode) })
    if (command.text) await session.send({ prompt: String(command.text) })
    tell('did', { n: command.n })
  } catch (error) {
    tell('did', { n: command.n, error: String((error && error.message) || error) })
  }
}

async function listen() {
  let quick = 0
  while (socket && quick < FAILURES) {
    const began = Date.now()
    let commands = []
    const reply = await post('/v1/next', {})
    try {
      if (reply && reply.ok && reply.text) commands = JSON.parse(reply.text).commands
    } catch (error) {
      commands = []
    }
    if (!Array.isArray(commands)) commands = []
    for (const command of commands) run(command)
    quick = commands.length === 0 && Date.now() - began < QUICK_MS ? quick + 1 : 0
  }
}

async function ask(body) {
  const reply = await post('/v1/ask', body)
  if (!reply || !reply.ok) return { closed: true }
  try {
    return reply.text ? JSON.parse(reply.text) : null
  } catch (error) {
    return { closed: true }
  }
}

// Copilot shows the same prompt in the terminal concurrently; either side can
// answer first. The host ends this wait once the question is gone, as when the
// terminal answered first.
async function wait(kind, request) {
  if (!socket) return null
  asks += 1
  const id = loaded + '-' + asks
  tell('ask', { ask: id, [kind]: brief(request, 0) })
  await settled()
  let answer = null
  let quick = 0
  while (socket && !answer && quick < FAILURES) {
    const began = Date.now()
    answer = await ask({ ask: id })
    quick = !answer && Date.now() - began < QUICK_MS ? quick + 1 : 0
  }
  return answer
}

async function decide(request) {
  const answer = await wait('request', request)
  return answer && answer.decision ? answer.decision : { kind: 'no-result' }
}

// Elicitation and plan-exit hooks have no "defer to terminal" result (unlike
// permissions' `no-result`), so returning a pending promise leaves the
// terminal in control. After the terminal's answer, Copilot takes no other.
const UNANSWERED = new Promise(() => {})

async function form(context) {
  const answer = await wait('form', { message: context.message, requestedSchema: context.requestedSchema })
  if (answer && answer.decision) return answer.decision
  return answer && answer.closed ? { action: 'cancel' } : UNANSWERED
}

// Approving autopilot in the terminal enables approve-all for the session;
// mirror that here when autopilot is chosen from Kodosi.
async function plan(request) {
  const answer = await wait('plan', request)
  const decision = answer && answer.decision
  if (decision && decision.approved && String(decision.selectedAction).startsWith('autopilot')) {
    try {
      await session.rpc.permissions.setApproveAll({ enabled: true, source: 'autopilot_confirmation' })
    } catch (error) {
      tell('did', { plan: decision.selectedAction, error: String(error) })
    }
  }
  if (decision) return decision
  return answer && answer.closed ? { approved: false } : UNANSWERED
}

if (socket && terminal) {
  session = await joinSession({
    events: { streaming: true },
    onPermissionRequest: decide,
    onElicitationRequest: form,
    onExitPlanModeRequest: plan,
  })
  const early = []
  let started = false
  session.on(event => {
    if (event && event.type === 'session.todos_changed' && started) readTasks()
    const shown = shaped(event)
    if (!shown) return
    if (started) tell('event', shown)
    else early.push(shown)
  })
  await hello(early)
  started = true
  for (const event of early.splice(0)) tell('event', event)
  listen()
}
