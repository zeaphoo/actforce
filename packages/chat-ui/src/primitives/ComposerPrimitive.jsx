import { forwardRef, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react'
import { ComposerContext, RuntimeContext } from '../runtime/context.js'

/**
 * Composer root. Manages input text and exposes submit() via context.
 * Submitting triggers `runtime.thread.append({ role: 'user', content: [{type:'text', text}] })`.
 */
const Root = forwardRef(function ComposerRoot(
  { children, className, style, onSubmit, ...rest }, ref
) {
  const runtime = useContext(RuntimeContext)
  const [text, setText] = useState('')

  const submit = useCallback(() => {
    const value = text.trim()
    if (!value) return
    const message = { role: 'user', content: [{ type: 'text', text: value }] }
    if (onSubmit) onSubmit(message)
    else if (runtime?.thread?.append) runtime.thread.append(message)
    setText('')
  }, [text, runtime, onSubmit])

  const ctx = useMemo(() => ({ text, setText, submit }), [text, submit])

  return (
    <ComposerContext.Provider value={ctx}>
      <div ref={ref} className={className} style={style} {...rest}>
        {children}
      </div>
    </ComposerContext.Provider>
  )
})
Root.displayName = 'ComposerPrimitive.Root'

/**
 * Controlled textarea. Enter submits, Shift+Enter adds a newline.
 * Auto-resizes to content up to `maxRows` rows.
 */
const Input = forwardRef(function ComposerInput(
  { className, rows = 1, maxRows = 8, onKeyDown, placeholder, ...rest }, ref
) {
  const ctx = useContext(ComposerContext)
  if (!ctx) throw new Error('ComposerPrimitive.Input must be inside ComposerPrimitive.Root')

  const textareaRef = useRef(null)

  // Auto-resize: adjust textarea height based on content, capped at maxRows
  useEffect(() => {
    const el = ref?.current || textareaRef.current
    if (!el) return
    // Reset height to auto first so shrinkage is detected
    el.style.height = 'auto'
    // Compute the max height based on maxRows
    const lineHeight = parseFloat(getComputedStyle(el).lineHeight) || 20
    const maxHeight = lineHeight * maxRows
    // Set height to scrollHeight, but cap at maxHeight
    const newHeight = Math.min(el.scrollHeight, maxHeight)
    el.style.height = `${newHeight}px`
    // If content exceeds maxHeight, enable scrollbar
    el.style.overflowY = el.scrollHeight > maxHeight ? 'auto' : 'hidden'
  }, [ctx.text, maxRows, ref])

  const handleKeyDown = (e) => {
    if (onKeyDown) onKeyDown(e)
    if (e.defaultPrevented) return
    if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault()
      ctx.submit()
    }
  }

  return (
    <textarea
      ref={ref || textareaRef}
      className={className}
      placeholder={placeholder}
      rows={rows}
      value={ctx.text}
      onChange={(e) => ctx.setText(e.target.value)}
      onKeyDown={handleKeyDown}
      {...rest}
    />
  )
})
Input.displayName = 'ComposerPrimitive.Input'

/** Submit button. Disabled when empty. */
const Send = forwardRef(function ComposerSend(
  { children, className, disabled, onClick, ...rest }, ref
) {
  const ctx = useContext(ComposerContext)
  const isEmpty = !ctx || ctx.text.trim() === ''
  return (
    <button
      ref={ref}
      type="button"
      className={className}
      disabled={disabled || isEmpty}
      onClick={(e) => {
        onClick?.(e)
        if (!e.defaultPrevented) ctx?.submit()
      }}
      {...rest}
    >
      {children}
    </button>
  )
})
Send.displayName = 'ComposerPrimitive.Send'

export const ComposerPrimitive = { Root, Input, Send }
