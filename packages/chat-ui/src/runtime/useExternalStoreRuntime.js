import { useEffect, useMemo, useRef } from 'react'
import { createStore } from './store.js'

/**
 * Bridge an externally-managed message list into a runtime object.
 *
 * @param {object} opts
 * @param {Array}  opts.messages       - array of message-like objects owned by the caller
 * @param {Function} [opts.setMessages] - setter (optional; enables runtime-driven mutations)
 * @param {boolean} [opts.isRunning]   - whether a response is currently streaming
 * @param {Function} [opts.convertMessage] - optional (msg) => ThreadMessage mapper
 * @param {Function} [opts.onNew]      - called when runtime.thread.append is invoked
 * @returns {{ thread: { append: Function }, _store: object }}
 */
export function useExternalStoreRuntime({
  messages,
  setMessages,
  isRunning = false,
  convertMessage,
  onNew,
}) {
  // Create the store only once per component lifetime.
  const storeRef = useRef(null)
  if (storeRef.current === null) {
    storeRef.current = createStore({
      messages: [],
      isRunning: false,
      copiedIds: new Set(),
    })
  }
  const store = storeRef.current

  // Keep the latest onNew in a ref so the returned runtime is stable.
  const onNewRef = useRef(onNew)
  useEffect(() => { onNewRef.current = onNew }, [onNew])

  // Sync incoming props into the store.
  const converted = useMemo(() => {
    if (!messages) return []
    return convertMessage ? messages.map((m, i) => convertMessage(m, i)) : messages
  }, [messages, convertMessage])

  useEffect(() => {
    store.setState((s) => {
      if (s.messages === converted && s.isRunning === isRunning) return s
      return { ...s, messages: converted, isRunning }
    })
  }, [store, converted, isRunning])

  // Stable runtime object.
  const runtime = useMemo(() => {
    const markCopied = (id, ms = 2000) => {
      store.setState((s) => {
        const copied = new Set(s.copiedIds)
        copied.add(id)
        return { ...s, copiedIds: copied }
      })
      setTimeout(() => {
        store.setState((s) => {
          if (!s.copiedIds.has(id)) return s
          const copied = new Set(s.copiedIds)
          copied.delete(id)
          return { ...s, copiedIds: copied }
        })
      }, ms)
    }

    return {
      _store: store,
      thread: {
        append: (message) => {
          const fn = onNewRef.current
          if (fn) return fn(message)
        },
      },
      _internal: { markCopied, setMessages },
    }
  }, [store, setMessages])

  return runtime
}
