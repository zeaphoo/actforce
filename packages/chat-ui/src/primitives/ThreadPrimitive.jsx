import {
  forwardRef,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react'
import { MessageContext, StoreContext, ThreadViewportContext } from '../runtime/context.js'
import { MessagePrimitiveRoot } from './MessagePrimitive.jsx'

function useStoreSnapshot() {
  const store = useContext(StoreContext)
  if (!store) throw new Error('ThreadPrimitive must be used inside <AssistantRuntimeProvider>.')
  return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot)
}

const Root = forwardRef(function ThreadRoot({ children, style, className, ...rest }, ref) {
  return (
    <div ref={ref} className={className} style={style} {...rest}>
      {children}
    </div>
  )
})

/**
 * Scrollable viewport. Tracks whether the user is near the bottom and auto-sticks
 * to bottom on new content while they're at the bottom.
 */
const Viewport = forwardRef(function ThreadViewport(
  { children, className, style, autoScroll = true, stickToBottomThreshold = 40, entryOffset = 0, ...rest },
  forwardedRef
) {
  const innerRef = useRef(null)
  const setRef = useCallback((node) => {
    innerRef.current = node
    if (typeof forwardedRef === 'function') forwardedRef(node)
    else if (forwardedRef) forwardedRef.current = node
  }, [forwardedRef])

  const state = useStoreSnapshot()
  const [isAtBottom, setIsAtBottom] = useState(true)
  const stickyRef = useRef(true)
  const didEntryRef = useRef(false)

  const checkAtBottom = useCallback(() => {
    const el = innerRef.current
    if (!el) return true
    const distance = el.scrollHeight - el.scrollTop - el.clientHeight
    return distance <= stickToBottomThreshold
  }, [stickToBottomThreshold])

  const scrollToBottom = useCallback((behavior = 'smooth') => {
    const el = innerRef.current
    if (!el) return
    el.scrollTo({ top: el.scrollHeight, behavior })
    stickyRef.current = true
    setIsAtBottom(true)
  }, [])

  // Track scroll position
  useEffect(() => {
    const el = innerRef.current
    if (!el) return
    const handler = () => {
      const atBottom = checkAtBottom()
      stickyRef.current = atBottom
      setIsAtBottom(atBottom)
    }
    el.addEventListener('scroll', handler, { passive: true })
    return () => el.removeEventListener('scroll', handler)
  }, [checkAtBottom])

  // Auto-scroll: streaming → stick to bottom; first load → entry animation
  useLayoutEffect(() => {
    if (!autoScroll) return
    const el = innerRef.current
    if (!el) return
    const max = el.scrollHeight - el.clientHeight
    if (max <= 0) return

    if (state.isRunning) {
      el.scrollTop = el.scrollHeight
      didEntryRef.current = true
      return
    }

    if (didEntryRef.current) {
      if (stickyRef.current) el.scrollTop = el.scrollHeight
      return
    }
    didEntryRef.current = true

    // First load with offset: jump near bottom, then smooth to end
    if (entryOffset === 'halfScreen') {
      const half = Math.round(el.clientHeight * 0.5)
      if (max > half) {
        el.scrollTop = max - half
        requestAnimationFrame(() => {
          el.scrollTo({ top: max, behavior: 'smooth' })
        })
        return
      }
    } else if (typeof entryOffset === 'number' && entryOffset > 0) {
      if (max > entryOffset) {
        el.scrollTop = max - entryOffset
        requestAnimationFrame(() => {
          el.scrollTo({ top: max, behavior: 'smooth' })
        })
        return
      }
    }

    el.scrollTop = el.scrollHeight
  }, [state.messages, state.isRunning, autoScroll, entryOffset])

  const ctx = useMemo(() => ({ isAtBottom, scrollToBottom }), [isAtBottom, scrollToBottom])

  return (
    <ThreadViewportContext.Provider value={ctx}>
      <div ref={setRef} className={className} style={style} {...rest}>
        {children}
      </div>
    </ThreadViewportContext.Provider>
  )
})

/**
 * Conditional render based on thread state. Supported props:
 *   empty, notEmpty, running, notRunning
 * Multiple props are AND-combined.
 */
function If({ children, empty, notEmpty, running, notRunning }) {
  const state = useStoreSnapshot()
  const isEmpty = state.messages.length === 0
  const isRun = !!state.isRunning

  if (empty != null && !!empty !== isEmpty) return null
  if (notEmpty != null && !!notEmpty !== !isEmpty) return null
  if (running != null && !!running !== isRun) return null
  if (notRunning != null && !!notRunning !== !isRun) return null

  return <>{children}</>
}

const DefaultUserMessage = () => null
const DefaultAssistantMessage = () => null
const DefaultSystemMessage = () => null

/**
 * Renders each message with the appropriate component from `components`.
 * Each rendered component sees the current message via `MessageContext`.
 */
function Messages({ components = {} }) {
  const state = useStoreSnapshot()
  const UserMessage = components.UserMessage || DefaultUserMessage
  const AssistantMessage = components.AssistantMessage || DefaultAssistantMessage
  const SystemMessage = components.SystemMessage || DefaultSystemMessage

  return (
    <>
      {state.messages.map((m) => {
        const Comp =
          m.role === 'user' ? UserMessage :
          m.role === 'system' ? SystemMessage :
          AssistantMessage

        return (
          <MessageContext.Provider key={m.id} value={m}>
            <Comp />
          </MessageContext.Provider>
        )
      })}
    </>
  )
}

/** Button that scrolls the viewport to the bottom; hidden when already at bottom. */
const ScrollToBottom = forwardRef(function ScrollToBottom(
  { children, className, hideWhenAtBottom = true, ...rest }, ref
) {
  const vp = useContext(ThreadViewportContext)
  if (!vp) return null
  if (hideWhenAtBottom && vp.isAtBottom) return null
  return (
    <button
      ref={ref}
      type="button"
      onClick={() => vp.scrollToBottom()}
      className={className}
      {...rest}
    >
      {children}
    </button>
  )
})

// Expose the MessagePrimitive.Root escape hatch used by some examples.
Root.displayName = 'ThreadPrimitive.Root'
Viewport.displayName = 'ThreadPrimitive.Viewport'
If.displayName = 'ThreadPrimitive.If'
Messages.displayName = 'ThreadPrimitive.Messages'
ScrollToBottom.displayName = 'ThreadPrimitive.ScrollToBottom'

export const ThreadPrimitive = {
  Root,
  Viewport,
  If,
  Messages,
  ScrollToBottom,
  // re-export for convenience so consumers can render a message root from here if desired
  MessageRoot: MessagePrimitiveRoot,
}
