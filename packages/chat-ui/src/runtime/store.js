// A tiny pub/sub store used by the runtime.
// State shape:
// {
//   messages: ThreadMessage[],   // converted messages (role, content, id, createdAt, metadata)
//   isRunning: boolean,
//   copiedIds: Set<string>,      // ephemeral: which message ids are currently showing "copied"
// }
export function createStore(initialState) {
  let state = initialState
  const listeners = new Set()

  const getSnapshot = () => state

  const setState = (updater) => {
    const next = typeof updater === 'function' ? updater(state) : updater
    if (next === state) return
    state = next
    listeners.forEach((l) => l())
  }

  const subscribe = (listener) => {
    listeners.add(listener)
    return () => listeners.delete(listener)
  }

  return { getSnapshot, setState, subscribe }
}

// Compute the per-message status (running/complete/incomplete).
// The last assistant message is "running" while isRunning is true; otherwise "complete".
export function computeMessageStatus(state, message) {
  if (!message) return { type: 'complete' }
  const messages = state.messages
  const isLast = messages.length > 0 && messages[messages.length - 1].id === message.id
  if (isLast && state.isRunning && message.role === 'assistant') {
    return { type: 'running' }
  }
  return { type: 'complete' }
}
