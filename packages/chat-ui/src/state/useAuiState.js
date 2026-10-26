import { useContext, useRef, useSyncExternalStore } from 'react'
import { MessageContext, MessagePartContext, StoreContext } from '../runtime/context.js'
import { computeMessageStatus } from '../runtime/store.js'

/**
 * Subscribe to a piece of runtime/message state. Mirrors @assistant-ui's `useAuiState`.
 *
 * The selector receives a snapshot of the form:
 *   {
 *     thread:  { isRunning, messages },
 *     message: { role, content, id, createdAt, metadata, status, isCopied },
 *     part:    { type, text, ... } | null,
 *   }
 *
 * `message` / `part` are only populated inside the respective render contexts.
 */
export function useAuiState(selector) {
  const store = useContext(StoreContext)
  const contextMessage = useContext(MessageContext)
  const contextPart = useContext(MessagePartContext)

  if (!store) {
    throw new Error('useAuiState must be used inside <AssistantRuntimeProvider>.')
  }

  // Cache the computed snapshot + selected value per (state, messageId, part).
  // useSyncExternalStore calls getSnapshot frequently and requires referential stability.
  const cacheRef = useRef({ state: null, msgId: null, part: null, selector: null, value: undefined })

  const getSnapshot = () => {
    const state = store.getSnapshot()
    const msgId = contextMessage?.id
    const c = cacheRef.current
    if (
      c.state === state &&
      c.msgId === msgId &&
      c.part === contextPart &&
      c.selector === selector
    ) {
      return c.value
    }

    // Prefer the freshest message object from the store; fall back to context.
    const liveMessage = msgId ? state.messages.find((m) => m.id === msgId) : null
    const baseMessage = liveMessage || contextMessage || null

    const snap = {
      thread: {
        isRunning: state.isRunning,
        messages: state.messages,
      },
      message: baseMessage
        ? {
            ...baseMessage,
            status: computeMessageStatus(state, baseMessage),
            isCopied: msgId ? state.copiedIds.has(msgId) : false,
          }
        : null,
      part: contextPart,
    }

    const value = selector(snap)
    cacheRef.current = { state, msgId, part: contextPart, selector, value }
    return value
  }

  return useSyncExternalStore(store.subscribe, getSnapshot, getSnapshot)
}
