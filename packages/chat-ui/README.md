# @actforce/chat-ui

A minimal, dependency-light chat UI primitive library. It re-implements only the
subset of [`@assistant-ui/react`](https://github.com/assistant-ui/assistant-ui)
that our apps actually use, with a near drop-in surface so existing call sites
need little or no change.

Designed to be consumed by both `web` and `web-mega`.

## What's included

Runtime:

- `AssistantRuntimeProvider`
- `useExternalStoreRuntime({ messages, setMessages, isRunning, convertMessage, onNew })`
- `useAuiState(selector)` — reads `thread` / `message` / `part` slices

Primitives:

- `ThreadPrimitive.{Root, Viewport, Messages, If, ScrollToBottom}`
- `MessagePrimitive.{Root, Content}` + `MessagePartPrimitive.{Text}`
- `ComposerPrimitive.{Root, Input, Send}`
- `ActionBarPrimitive.{Root, Copy}`

Markdown:

- `MarkdownTextPrimitive` — built on `react-markdown` + `remark-gfm`, with:
  - **Syntax highlighting** via `rehype-highlight` (default on). Import the theme CSS:
    ```js
    import '@actforce/chat-ui/markdown.css'
    ```
  - **Smooth streaming** (default on while the surrounding message is `running`):
    renders the text character-by-character instead of in bursts.
    Control via the `smooth` prop: `smooth={false}` to disable, `smooth={180}` to set chars/sec.
- `SmoothText` — same smoothing without markdown parsing.
- `useSmoothText(target, { charsPerSecond, enabled })` — the underlying hook.

## Intentionally **not** included

Everything else from `@assistant-ui/react`: built-in runtime adapters, branch /
edit composer, attachment adapter, tool call UI, reasoning parts, suggestions
primitive, speech, etc. Build these in your application when you need them.

## Install

This package is not published; consume it via a relative/workspace reference.
Examples:

```jsonc
// web/package.json
{
  "dependencies": {
    "@actforce/chat-ui": "file:../../packages/chat-ui"
  }
}
```

```jsonc
// web-mega/pnpm-workspace.yaml adjustment + package.json
{
  "dependencies": {
    "@actforce/chat-ui": "workspace:*"
  }
}
```

Peer deps: `react >= 18`, `react-dom >= 18`.

## Quick start

```jsx
import {
  AssistantRuntimeProvider, useExternalStoreRuntime,
  ThreadPrimitive, MessagePrimitive, ComposerPrimitive,
  ActionBarPrimitive, MarkdownTextPrimitive, useAuiState,
} from '@actforce/chat-ui'

function Assistant() {
  const isRunning = useAuiState(s => s.message.status?.type === 'running')
  return (
    <MessagePrimitive.Root>
      <MessagePrimitive.Content components={{ Text: MarkdownTextPrimitive }} />
      {isRunning && <span>…</span>}
      <ActionBarPrimitive.Root>
        <ActionBarPrimitive.Copy>Copy</ActionBarPrimitive.Copy>
      </ActionBarPrimitive.Root>
    </MessagePrimitive.Root>
  )
}

export default function App() {
  const [messages, setMessages] = useState([])
  const [isRunning, setIsRunning] = useState(false)

  const runtime = useExternalStoreRuntime({
    messages, setMessages, isRunning,
    onNew: async (msg) => { /* send to backend, push assistant message */ },
  })

  return (
    <AssistantRuntimeProvider runtime={runtime}>
      <ThreadPrimitive.Root>
        <ThreadPrimitive.Viewport>
          <ThreadPrimitive.If empty><Welcome /></ThreadPrimitive.If>
          <ThreadPrimitive.Messages components={{ UserMessage, AssistantMessage: Assistant }} />
          <ThreadPrimitive.ScrollToBottom>↓</ThreadPrimitive.ScrollToBottom>
        </ThreadPrimitive.Viewport>
        <ComposerPrimitive.Root>
          <ComposerPrimitive.Input placeholder="Ask..." />
          <ComposerPrimitive.Send>Send</ComposerPrimitive.Send>
        </ComposerPrimitive.Root>
      </ThreadPrimitive.Root>
    </AssistantRuntimeProvider>
  )
}
```

## Migration from `@assistant-ui/react`

Most imports can be swapped 1:1. Known differences:

| Feature | Status |
|---|---|
| `useExternalStoreRuntime({ messages, setMessages, isRunning, convertMessage, onNew })` | Supported |
| `runtime.thread.append(message)` | Supported |
| `useAuiState(s => s.message.{role,content,status,isCopied})` | Supported |
| `ThreadPrimitive.If` | Supports `empty`/`notEmpty`/`running`/`notRunning` only |
| `MessagePrimitive.Content` `components` | Supports `Text`, `Image`, and custom part types |
| `ActionBarPrimitive.Copy` | Supported (flips `message.isCopied` for 2s) |
| Attachments, branches, edit composer, tool UI, etc. | **Not included** — implement in app layer |

## Develop / demo

```bash
cd packages/chat-ui
pnpm install  # or npm install
pnpm example  # starts a minimal Vite demo on :5180
```
