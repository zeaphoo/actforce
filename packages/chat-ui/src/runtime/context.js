import { createContext, useContext } from 'react'

// Runtime (returned by useExternalStoreRuntime), provides `thread.append` etc.
export const RuntimeContext = createContext(null)

// Internal store for reactive state (messages, isRunning, copiedIds).
export const StoreContext = createContext(null)

// The current message object while rendering a message subtree.
export const MessageContext = createContext(null)

// The current message part while rendering parts (used by MarkdownTextPrimitive etc.).
export const MessagePartContext = createContext(null)

// Composer local state (text + submit).
export const ComposerContext = createContext(null)

// Thread viewport context (for ScrollToBottom button).
export const ThreadViewportContext = createContext(null)

export const useRuntime = () => {
  const r = useContext(RuntimeContext)
  if (!r) throw new Error('useRuntime must be used inside <AssistantRuntimeProvider>.')
  return r
}

export const useStore = () => {
  const s = useContext(StoreContext)
  if (!s) throw new Error('Store is not available. Wrap your tree in <AssistantRuntimeProvider>.')
  return s
}

export const useMessage = () => useContext(MessageContext)
export const useMessagePart = () => useContext(MessagePartContext)
export const useComposer = () => useContext(ComposerContext)
export const useThreadViewport = () => useContext(ThreadViewportContext)
