import { useContext, useMemo, createContext } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter'
import { oneDark } from 'react-syntax-highlighter/dist/esm/styles/prism'
import { MessagePartContext } from '../runtime/context.js'
import { useAuiState } from '../state/useAuiState.js'
import { useSmoothText } from './useSmoothText.js'

// Context to pass pre props from PreOverride to CodeBlock (like assistant-ui)
const PreContext = createContext(null)

/**
 * Renders the current text part as GitHub-flavored markdown with optional
 * syntax highlighting and smooth streaming.
 *
 * Props:
 *   components        — per-element overrides forwarded to react-markdown
 *   remarkPlugins     — override default `[remarkGfm]`
 *   highlight         — enable syntax highlighting (default true).
 *                       Remember to import '@actforce/chat-ui/markdown.css' for the theme.
 *   smooth            — animate character-by-character while the message is streaming
 *                       (default true). Pass `false` to disable or a number to set
 *                       characters/second.
 *   className         — passed to the rendered markdown root
 */
export function MarkdownTextPrimitive({
  components,
  remarkPlugins,
  highlight = true,
  smooth = true,
  className,
}) {
  const part = useContext(MessagePartContext)
  const raw = part?.type === 'text' ? part.text || '' : ''

  // Only animate while the surrounding message is actively streaming.
  const isRunning = useAuiState((s) => s.message?.status?.type === 'running')
  const smoothEnabled = !!smooth && isRunning
  const charsPerSecond = typeof smooth === 'number' ? smooth : 120
  const text = useSmoothText(raw, { enabled: smoothEnabled, charsPerSecond })

  const plugins = useMemo(() => {
    const r = remarkPlugins ?? [remarkGfm]
    return { r }
  }, [remarkPlugins])

  // Code block component with syntax highlighting (like assistant-ui CodeOverride)
  const CodeBlock = useMemo(() => {
    if (!highlight) return undefined
    
    return ({ node, inline, className, children, ...props }) => {
      // Check if we're inside a code block (from PreOverride context)
      const preProps = useContext(PreContext)
      const isCodeBlock = preProps !== null
      
      // If not in a code block context, render as inline code
      if (!isCodeBlock) {
        return (
          <code className={className} {...props}>
            {children}
          </code>
        )
      }
      
      const match = /language-(\w+)/.exec(className || '')
      const language = match ? match[1] : 'text'
      const code = String(children).replace(/\n$/, '')
      
      // Render SyntaxHighlighter with the Pre component from context
      // This ensures <pre> is rendered at the correct DOM level, not inside <p>
      return (
        <SyntaxHighlighter
          style={oneDark}
          language={language}
          PreTag={({ children: preChildren, ...preProps }) => (
            <pre {...preProps}>{preChildren}</pre>
          )}
          {...props}
        >
          {code}
        </SyntaxHighlighter>
      )
    }
  }, [highlight])

  return (
    <div className={className}>
      <ReactMarkdown
        remarkPlugins={plugins.r}
        components={{
          // PreOverride: provide context instead of rendering pre directly
          pre({ children, ...preProps }) {
            return (
              <PreContext.Provider value={preProps}>
                {children}
              </PreContext.Provider>
            )
          },
          code: CodeBlock,
          ...components,
        }}
        urlTransform={null}
      >
        {text}
      </ReactMarkdown>
    </div>
  )
}
MarkdownTextPrimitive.displayName = 'MarkdownTextPrimitive'

/**
 * Smooth text without markdown parsing. Useful as a drop-in `Text` component
 * when you only want the streaming animation.
 */
export function SmoothText({ smooth = true, className, as: Tag = 'span' }) {
  const part = useContext(MessagePartContext)
  const raw = part?.type === 'text' ? part.text || '' : ''
  const isRunning = useAuiState((s) => s.message?.status?.type === 'running')
  const enabled = !!smooth && isRunning
  const charsPerSecond = typeof smooth === 'number' ? smooth : 120
  const text = useSmoothText(raw, { enabled, charsPerSecond })
  return <Tag className={className}>{text}</Tag>
}
SmoothText.displayName = 'SmoothText'
