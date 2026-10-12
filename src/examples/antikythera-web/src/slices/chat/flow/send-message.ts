import { ref } from 'vue'
import { createAgentRuntime, type ClientCoreRuntime } from 'antikythera-agent/runtime'
import { CLIENT_TOOL_ENTRIES } from '@/shared/adapters/tool-adapter'
import { getModel } from '@/shared/adapters/model-store'
import { eventBus } from '@/shared/bus/event-bus'
import { createMessage, type Message, type ToolCallRecord } from '../core/message'
import type { ChatStatus } from '../core/state'

/**
 * The server runtime binary binds 127.0.0.1:8787 by default
 * (antikythera-server-runtime/src/config.rs); the URL is configurable via VITE_SERVER_URL or
 * localStorage before the first runtime creation.
 */
const DEFAULT_SERVER_URL = 'http://127.0.0.1:8787'

const messages = ref<Message[]>([])
const status = ref<ChatStatus>('idle')
const sessionId = ref<string | null>(null)
const streamingContent = ref('')
const error = ref<string | null>(null)

let runtime: ClientCoreRuntime | null = null
let runtimePromise: Promise<ClientCoreRuntime> | null = null
let toolCallsForTurn: ToolCallRecord[] = []
let toolRoundActive = false

function resolveServerUrl(): string {
  const fromEnv = import.meta.env.VITE_SERVER_URL
  const fromStorage = localStorage.getItem('antikythera_server_url')
  return (typeof fromEnv === 'string' && fromEnv ? fromEnv : fromStorage) || DEFAULT_SERVER_URL
}

function toolProgressText(calls: ToolCallRecord[]): string {
  return calls
    .map((call) => (call.success ? `✓ ${call.tool}` : `✗ ${call.tool}`))
    .join(' | ')
}

/**
 * Forward runtime events to the chat UI. Tool progress is driven by the
 * runtime's `tool_requested`/`tool_result` events; token streaming by
 * `llm-token`. The tool-call JSON draft an LLM emits before a tool round is
 * hidden from the stream (toolRoundActive resets the buffer on the next
 * token burst).
 */
function installEventForwarding(rt: ClientCoreRuntime): void {
  rt.onEvent((event) => {
    switch (event.type) {
      case 'llm-token': {
        const chunk = event.chunk
        if (typeof chunk === 'string' && chunk) {
          if (toolRoundActive) {
            toolRoundActive = false
            streamingContent.value = ''
          }
          streamingContent.value += chunk
        }
        break
      }
      case 'tool_requested': {
        toolRoundActive = true
        streamingContent.value = ''
        toolCallsForTurn.push({
          tool: typeof event.tool === 'string' ? event.tool : String(event.tool),
          input: (event.input as Record<string, unknown> | undefined) ?? {},
          output: null,
          success: false,
        })
        break
      }
      case 'tool_result': {
        const last = toolCallsForTurn[toolCallsForTurn.length - 1]
        if (last) {
          last.success = event.success === true
        }
        streamingContent.value = toolProgressText(toolCallsForTurn)
        break
      }
      case 'error': {
        error.value = typeof event.error === 'string' ? event.error : String(event.error)
        break
      }
    }
  })
}

function ensureRuntime(): Promise<ClientCoreRuntime> {
  if (runtime) return Promise.resolve(runtime)
  if (!runtimePromise) {
    runtimePromise = createAgentRuntime({
      core: 'client',
      serverUrl: resolveServerUrl(),
      tools: CLIENT_TOOL_ENTRIES,
      policy: { allow: CLIENT_TOOL_ENTRIES.map((entry) => entry.definition.name) },
      llm: { model: getModel() },
      maxSteps: 10,
    })
      .then((rt) => {
        runtime = rt
        installEventForwarding(rt)
        return rt
      })
      .catch((err: unknown) => {
        runtimePromise = null
        throw err
      })
  }
  return runtimePromise
}

/**
 * Enrich event-level tool records with the output the runner stored in the
 * session state (`tool_results`), which is the only place the runtime
 * exposes the parsed tool output to the host.
 */
function finalizeToolCalls(): ToolCallRecord[] {
  if (toolCallsForTurn.length === 0) return []
  let toolResults: Record<string, unknown> = {}
  try {
    const state = runtime?.getState()
    const results = state?.['tool_results']
    if (results && typeof results === 'object') {
      toolResults = results as Record<string, unknown>
    }
  } catch {
    toolResults = {}
  }
  return toolCallsForTurn.map((record) => ({
    ...record,
    output: record.success
      ? ((toolResults[record.tool] as Record<string, unknown>) ?? null)
      : null,
  }))
}

export function useChat() {
  async function initSession_() {
    const rt = await ensureRuntime()
    if (!rt.connected) {
      await rt.connect()
    }
    sessionId.value = rt.sessionId
    eventBus.emit('session:created', {
      sessionId: rt.sessionId ?? 'unknown',
      title: 'New Chat',
    })
  }

  async function sendMessage(content: string) {
    messages.value.push(createMessage('user', content))
    status.value = 'streaming'
    streamingContent.value = ''
    error.value = null
    toolCallsForTurn = []

    eventBus.emit('chat:message-sent', {
      sessionId: sessionId.value ?? 'unknown',
      content,
      timestamp: Date.now(),
    })

    try {
      if (!runtime || !runtime.connected) {
        await initSession_()
      }
      const result = await runtime!.runTurn(content)

      streamingContent.value = ''
      const assistantMsg = createMessage(
        'assistant',
        result.content ?? '',
        finalizeToolCalls(),
      )
      messages.value.push(assistantMsg)
      status.value = 'idle'

      eventBus.emit('chat:response-completed', {
        sessionId: result.sessionId,
        content: result.content ?? '',
      })
    } catch (e: unknown) {
      console.error('[Chat] ERROR:', e)
      /**
       * A failed turn may leave the runner mid-stream; drop the session so the next message
       * reconnects with a fresh runner session.
       */
      try {
        runtime?.resetSession()
      } catch {
        /**
         * Reset failure is non-fatal; the next connect attempt re-inits.
         */
      }
      sessionId.value = null
      status.value = 'error'
      error.value = e instanceof Error ? e.message : String(e)
      eventBus.emit('chat:error-occurred', {
        sessionId: sessionId.value ?? 'unknown',
        error: error.value,
      })
    }
  }

  function clearMessages() {
    messages.value = []
    streamingContent.value = ''
    error.value = null
  }

  function switchSession(newSessionId: string) {
    /**
     * The runtime owns exactly one runner session; a switch request resets the local transcript
     * and the runner session for a fresh conversation.
     */
    try {
      runtime?.resetSession()
    } catch {
    }
    sessionId.value = null
    messages.value = []
    streamingContent.value = ''
    error.value = null
    status.value = 'idle'
    eventBus.emit('session:switched', { sessionId: newSessionId })
  }

  return {
    messages,
    status,
    sessionId,
    streamingContent,
    error,
    sendMessage,
    clearMessages,
    switchSession,
    initSession: initSession_,
  }
}
