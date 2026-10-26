import { forwardRef, useContext } from 'react'
import { MessageContext, RuntimeContext } from '../runtime/context.js'

const Root = forwardRef(function ActionBarRoot(
  { children, className, style, ...rest }, ref
) {
  return (
    <div ref={ref} className={className} style={style} role="toolbar" {...rest}>
      {children}
    </div>
  )
})
Root.displayName = 'ActionBarPrimitive.Root'

function extractText(message) {
  if (!message?.content) return ''
  if (typeof message.content === 'string') return message.content
  if (!Array.isArray(message.content)) return ''
  return message.content
    .filter((p) => p && p.type === 'text')
    .map((p) => p.text || '')
    .join('')
}

/**
 * Copy button. Copies the current message's text to the clipboard and briefly
 * flips `s.message.isCopied` in the store so consumers can swap icons.
 */
const Copy = forwardRef(function ActionBarCopy(
  { children, className, copiedDuration = 2000, onClick, ...rest }, ref
) {
  const runtime = useContext(RuntimeContext)
  const message = useContext(MessageContext)

  const handleClick = async (e) => {
    onClick?.(e)
    if (e.defaultPrevented || !message) return
    const text = extractText(message)
    try {
      if (navigator?.clipboard?.writeText) {
        await navigator.clipboard.writeText(text)
      } else {
        // Fallback for insecure contexts.
        const ta = document.createElement('textarea')
        ta.value = text
        ta.style.position = 'fixed'
        ta.style.opacity = '0'
        document.body.appendChild(ta)
        ta.select()
        document.execCommand('copy')
        document.body.removeChild(ta)
      }
      runtime?._internal?.markCopied?.(message.id, copiedDuration)
    } catch (err) {
      console.error('[chat-ui] copy failed', err)
    }
  }

  return (
    <button
      ref={ref}
      type="button"
      className={className}
      onClick={handleClick}
      aria-label="Copy message"
      {...rest}
    >
      {children}
    </button>
  )
})
Copy.displayName = 'ActionBarPrimitive.Copy'

export const ActionBarPrimitive = { Root, Copy }
