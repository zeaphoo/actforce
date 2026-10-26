import { useCallback, useState } from 'react'
import {
  AssistantRuntimeProvider,
  useExternalStoreRuntime,
  ThreadPrimitive,
  MessagePrimitive,
  ComposerPrimitive,
  ActionBarPrimitive,
  MarkdownTextPrimitive,
  useAuiState,
} from '../src/index.js'

const styles = {
  wrap: { display: 'flex', flexDirection: 'column', height: '100%', maxWidth: 780, margin: '0 auto' },
  viewport: { flex: 1, overflowY: 'auto', padding: 16, display: 'flex', flexDirection: 'column', position: 'relative' },
  empty: { margin: 'auto', textAlign: 'center', color: '#777' },
  row: { display: 'flex', gap: 12, padding: '10px 0' },
  userRow: { justifyContent: 'flex-end' },
  bubble: { padding: '8px 12px', borderRadius: 12, maxWidth: '80%', lineHeight: 1.5, whiteSpace: 'pre-wrap' },
  userBubble: { background: '#2563eb', color: '#fff', borderTopRightRadius: 2 },
  asstBubble: { background: 'rgba(127,127,127,.15)', borderTopLeftRadius: 2 },
  composer: { display: 'flex', gap: 8, padding: 12, borderTop: '1px solid rgba(127,127,127,.25)' },
  input: { flex: 1, resize: 'none', padding: 10, borderRadius: 8, border: '1px solid rgba(127,127,127,.35)', background: 'transparent', color: 'inherit', font: 'inherit' },
  send: { padding: '0 16px', borderRadius: 8, border: 'none', background: '#2563eb', color: '#fff', cursor: 'pointer' },
  actionBtn: { marginTop: 4, padding: '2px 8px', fontSize: 12, borderRadius: 6, border: '1px solid rgba(127,127,127,.35)', background: 'transparent', color: 'inherit', cursor: 'pointer' },
  scrollBtn: { position: 'absolute', bottom: 12, left: '50%', transform: 'translateX(-50%)', padding: '4px 10px', borderRadius: 999, border: '1px solid rgba(127,127,127,.35)', background: 'var(--card,#fff)', cursor: 'pointer' },
}

function UserMessage() {
  const text = useAuiState((s) => s.message.content?.[0]?.text || '')
  const isCopied = useAuiState((s) => s.message.isCopied)
  return (
    <MessagePrimitive.Root style={{ ...styles.row, ...styles.userRow }}>
      <div>
        <div style={{ ...styles.bubble, ...styles.userBubble }}>{text}</div>
        <ActionBarPrimitive.Root style={{ display: 'flex', justifyContent: 'flex-end' }}>
          <ActionBarPrimitive.Copy style={styles.actionBtn}>
            {isCopied ? '✓ Copied' : 'Copy'}
          </ActionBarPrimitive.Copy>
        </ActionBarPrimitive.Root>
      </div>
    </MessagePrimitive.Root>
  )
}

function AssistantMessage() {
  const isRunning = useAuiState((s) => s.message.status?.type === 'running')
  const isCopied = useAuiState((s) => s.message.isCopied)
  return (
    <MessagePrimitive.Root style={styles.row}>
      <div>
        <div style={{ ...styles.bubble, ...styles.asstBubble }}>
          <MessagePrimitive.Content components={{ Text: MarkdownTextPrimitive }} />
          {isRunning && <span style={{ opacity: 0.5 }}> ▍</span>}
        </div>
        {!isRunning && (
          <ActionBarPrimitive.Root>
            <ActionBarPrimitive.Copy style={styles.actionBtn}>
              {isCopied ? '✓ Copied' : 'Copy'}
            </ActionBarPrimitive.Copy>
          </ActionBarPrimitive.Root>
        )}
      </div>
    </MessagePrimitive.Root>
  )
}

export default function App() {
  const [messages, setMessages] = useState([])
  const [isRunning, setIsRunning] = useState(false)

  const onNew = useCallback(async (msg) => {
    const userText = msg.content?.[0]?.text || ''
    const userMsg = { id: `u-${Date.now()}`, role: 'user', content: [{ type: 'text', text: userText }] }
    const asstId = `a-${Date.now() + 1}`
    const asstMsg = { id: asstId, role: 'assistant', content: [{ type: 'text', text: '' }] }
    setMessages((m) => [...m, userMsg, asstMsg])
    setIsRunning(true)

    // Fake bursty streaming to exercise smooth-text animation.
    const reply = [
      `Echo: **${userText}**`,
      ``,
      `Markdown + code highlight + smooth streaming:`,
      ``,
      '```js',
      `function greet(name) {`,
      `  // a tiny demo`,
      `  const msg = \`hello, \${name}!\``,
      `  return msg.toUpperCase()`,
      `}`,
      '```',
      ``,
      `- inline \`code\` too`,
      `- [a link](https://example.com)`,
    ].join('\n')

    // Send in chunks of ~20 chars with irregular delays.
    let pos = 0
    while (pos < reply.length) {
      const step = 10 + Math.floor(Math.random() * 30)
      pos = Math.min(reply.length, pos + step)
      setMessages((m) => m.map((x) => x.id === asstId
        ? { ...x, content: [{ type: 'text', text: reply.slice(0, pos) }] }
        : x))
      await new Promise((r) => setTimeout(r, 60 + Math.random() * 80))
    }
    setIsRunning(false)
  }, [])

  const runtime = useExternalStoreRuntime({ messages, setMessages, isRunning, onNew })

  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <div style={styles.wrap}>
        <ThreadPrimitive.Root style={{ display: 'flex', flexDirection: 'column', flex: 1 }}>
          <ThreadPrimitive.Viewport style={styles.viewport}>
            <ThreadPrimitive.If empty>
              <div style={styles.empty}>
                <h2>chat-ui example</h2>
                <p>Type a message below to start.</p>
              </div>
            </ThreadPrimitive.If>
            <ThreadPrimitive.Messages components={{ UserMessage, AssistantMessage }} />
            <ThreadPrimitive.ScrollToBottom style={styles.scrollBtn}>↓ Latest</ThreadPrimitive.ScrollToBottom>
          </ThreadPrimitive.Viewport>

          <ComposerPrimitive.Root style={styles.composer}>
            <ComposerPrimitive.Input rows={2} placeholder="Type a message... (Enter to send, Shift+Enter newline)" style={styles.input} />
            <ComposerPrimitive.Send style={styles.send}>Send</ComposerPrimitive.Send>
          </ComposerPrimitive.Root>
        </ThreadPrimitive.Root>
      </div>
    </AssistantRuntimeProvider>
  )
}
