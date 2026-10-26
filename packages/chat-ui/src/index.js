// Runtime
export { AssistantRuntimeProvider } from './runtime/AssistantRuntimeProvider.jsx'
export { useExternalStoreRuntime } from './runtime/useExternalStoreRuntime.js'

// State hook
export { useAuiState } from './state/useAuiState.js'

// Primitives
export { ThreadPrimitive } from './primitives/ThreadPrimitive.jsx'
export { MessagePrimitive, MessagePartPrimitive } from './primitives/MessagePrimitive.jsx'
export { ComposerPrimitive } from './primitives/ComposerPrimitive.jsx'
export { ActionBarPrimitive } from './primitives/ActionBarPrimitive.jsx'

// Markdown (re-exported from subpath too)
export { MarkdownTextPrimitive, SmoothText } from './markdown/MarkdownTextPrimitive.jsx'
export { useSmoothText } from './markdown/useSmoothText.js'

// Low-level context hooks (useful for custom components)
export {
  useRuntime,
  useMessage,
  useMessagePart,
  useComposer,
  useThreadViewport,
} from './runtime/context.js'

// Context objects (for availability checks)
export { StoreContext } from './runtime/context.js'
