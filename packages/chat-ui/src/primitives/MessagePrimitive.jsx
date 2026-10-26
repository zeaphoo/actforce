import { forwardRef, useContext } from 'react'
import { MessageContext, MessagePartContext } from '../runtime/context.js'

export const MessagePrimitiveRoot = forwardRef(function MessageRoot(
  { children, className, style, ...rest }, ref
) {
  const message = useContext(MessageContext)
  return (
    <div
      ref={ref}
      className={className}
      style={style}
      data-message-id={message?.id}
      data-message-role={message?.role}
      {...rest}
    >
      {children}
    </div>
  )
})
MessagePrimitiveRoot.displayName = 'MessagePrimitive.Root'

function DefaultText() {
  const part = useContext(MessagePartContext)
  return <>{part?.text ?? ''}</>
}

function DefaultImage() {
  const part = useContext(MessagePartContext)
  if (!part?.image) return null
  return <img src={part.image} alt={part.alt || ''} />
}

/**
 * Renders each part of the current message.
 * `components` allows overriding rendering per part type:
 *   { Text: ..., Image: ..., [customType]: ... }
 */
function MessagePrimitiveContent({ components = {} }) {
  const message = useContext(MessageContext)
  if (!message) return null
  const parts = Array.isArray(message.content) ? message.content : []

  const Text = components.Text || DefaultText
  const Image = components.Image || DefaultImage

  return (
    <>
      {parts.map((part, idx) => {
        let Comp
        if (part.type === 'text') Comp = Text
        else if (part.type === 'image') Comp = Image
        else Comp = components[part.type] || (() => null)
        return (
          <MessagePartContext.Provider key={idx} value={part}>
            <Comp />
          </MessagePartContext.Provider>
        )
      })}
    </>
  )
}
MessagePrimitiveContent.displayName = 'MessagePrimitive.Content'

export const MessagePrimitive = {
  Root: MessagePrimitiveRoot,
  Content: MessagePrimitiveContent,
}

// Standalone exports for parts (mirrors @assistant-ui's MessagePartPrimitive namespace).
function MessagePartText() {
  const part = useContext(MessagePartContext)
  return <>{part?.type === 'text' ? part.text : ''}</>
}
MessagePartText.displayName = 'MessagePartPrimitive.Text'

export const MessagePartPrimitive = {
  Text: MessagePartText,
}
