import { create } from 'zustand'
import { getThreadMessages, createConversation, getInferenceHealth } from './api'
import { getAuthHeaders, getStoredIdentity } from './auth'

const defaultThread = {
  messages: [],
  isRunning: false,
  stepStatuses: {},
  streamingMessageId: null,
  isQueued: false,
  queuePosition: 0,
  startedAt: null,
  isLoading: false,
  initialized: false,
}

// 每个 threadId 的 init 取消控制器
const initControllers = new Map()

// ========== 共享 WebSocket 连接（多路复用：一个 identity 一个连接） ==========
let sharedWS = null
let sharedWSIdentityId = null
let wsReadyCallbacks = []
let wsReconnectTimer = null
const flushTimers = new Map()
const pendingContent = new Map()

function getWsUrl() {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${protocol}//${window.location.host}/api/chat/ws`
}

export function openSharedWS(identityId) {
  if (sharedWS?.readyState === WebSocket.OPEN || sharedWS?.readyState === WebSocket.CONNECTING) {
    if (sharedWSIdentityId === identityId) return sharedWS
    closeSharedWS()
  }
  sharedWSIdentityId = identityId
  const url = `${getWsUrl()}?identity_id=${encodeURIComponent(identityId)}`
  const ws = new WebSocket(url)
  sharedWS = ws

  ws.onopen = () => {
    wsReadyCallbacks.forEach(cb => cb.resolve(ws))
    wsReadyCallbacks = []
  }

  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data)
      const cid = data.conversation_id
      if (cid) handleWSMessage(cid, data)
    } catch {}
  }

  ws.onerror = () => {}

  ws.onclose = () => {
    sharedWS = null
    wsReadyCallbacks.forEach(cb => cb.reject(new Error('WS closed')))
    wsReadyCallbacks = []
    // 自动重连
    const runningIds = _s().getRunningThreadIds()
    if (runningIds.length > 0) {
      wsReconnectTimer = setTimeout(() => {
        openSharedWS(sharedWSIdentityId)
        // 重连后重新 sync 所有运行中的 thread
        awaitSharedWSOpen().then(() => {
          const store = _s()
          for (const tid of store.getRunningThreadIds()) {
            sendSharedWS({ type: 'sync', conversation_id: tid })
          }
        }).catch(() => {})
      }, 2000)
    }
  }

  return ws
}

function closeSharedWS() {
  clearTimeout(wsReconnectTimer)
  wsReconnectTimer = null
  if (sharedWS) {
    sharedWS.onopen = null
    sharedWS.onmessage = null
    sharedWS.onclose = null
    sharedWS.onerror = null
    if (sharedWS.readyState === WebSocket.OPEN || sharedWS.readyState === WebSocket.CONNECTING) {
      sharedWS.close()
    }
    sharedWS = null
  }
  wsReadyCallbacks.forEach(cb => cb.reject(new Error('WS closed')))
  wsReadyCallbacks = []
}

export function sendSharedWS(data) {
  if (sharedWS?.readyState === WebSocket.OPEN) {
    sharedWS.send(JSON.stringify(data))
    return true
  }
  return false
}

export function awaitSharedWSOpen(timeout = 5000) {
  if (sharedWS?.readyState === WebSocket.OPEN) return Promise.resolve(sharedWS)
  if (!sharedWS || sharedWS.readyState >= WebSocket.CLOSING) return Promise.reject(new Error('WS not connecting'))
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      const idx = wsReadyCallbacks.findIndex(cb => cb.reject === reject)
      if (idx >= 0) wsReadyCallbacks.splice(idx, 1)
      reject(new Error('WS connect timeout'))
    }, timeout)
    wsReadyCallbacks.push({
      resolve: (ws) => { clearTimeout(timer); resolve(ws) },
      reject,
    })
  })
}

// ========== WS 消息处理 ==========

function _writeMsgs(threadId, updater) {
  _s().setThreadMessages(threadId, updater)
}

function handleWSMessage(threadId, event) {
  const store = _s()

  switch (event.type) {
    case 'init':
      store.setThreadStreamingMessageId(threadId, event.streaming_message_id)
      break

    case 'user_message':
      if (event.message) {
        const bm = event.message
        const streamingId = store.threads[threadId]?.streamingMessageId
        const assistantId = streamingId || `ms-${Date.now()}`
        if (!streamingId) store.setThreadStreamingMessageId(threadId, assistantId)

        _writeMsgs(threadId, prev => {
          const hasUser = prev.some(m => m.id === event.user_message_id)
          let msgs = hasUser ? prev : [...prev, {
            role: 'user',
            content: [{ type: 'text', text: bm.content }],
            id: event.user_message_id || `user-${Date.now()}`,
            createdAt: new Date(),
            metadata: bm.attachments?.length > 0 ? { custom: { attachments: bm.attachments.map(att => ({
              name: att.name, type: att.type, size: att.size,
              path: att.path, url: att.url || `/api/chat/files/${att.path}`,
            })) } } : undefined,
          }]
          const hasAssistant = msgs.some(m => m.id === assistantId)
          return hasAssistant ? msgs : [...msgs, {
            role: 'assistant', content: [{ type: 'text', text: '' }],
            id: assistantId, createdAt: new Date(),
          }]
        })
      }
      break

    case 'token': {
      const text = event.text || ''
      const cur = pendingContent.get(threadId) || ''
      pendingContent.set(threadId, cur + text)
      if (!flushTimers.has(threadId)) {
        flushTimers.set(threadId, setTimeout(() => {
          flushTimers.delete(threadId)
          const content = pendingContent.get(threadId) || ''
          const sid = store.threads[threadId]?.streamingMessageId
          if (sid) {
            _writeMsgs(threadId, prev => prev.map(m =>
              m.id === sid ? { ...m, content: [{ type: 'text', text: content }] } : m
            ))
          }
        }, 1000))
      }
      break
    }

    case 'step_status':
      if (event.status) {
        store.setThreadStepStatuses(threadId, {
          ...store.threads[threadId]?.stepStatuses ?? {},
          [event.status.id]: event.status,
        })
      }
      break

    case 'attachments':
      if (Array.isArray(event.attachments)) {
        const sid = store.threads[threadId]?.streamingMessageId
        if (sid) {
          _writeMsgs(threadId, prev => prev.map(m => {
            if (m.id === sid) {
              const existing = m.metadata?.custom?.attachments || []
              return { ...m, metadata: { custom: { attachments: [...existing, ...event.attachments] } } }
            }
            return m
          }))
        }
      }
      break

    case 'queued':
      store.setThreadQueueState(threadId, true, event.position ?? 1)
      break

    case 'processing':
      store.setThreadQueueState(threadId, false, 0)
      break

    case 'done': {
      const timer = flushTimers.get(threadId)
      if (timer) { clearTimeout(timer); flushTimers.delete(threadId) }
      const content = event.content || pendingContent.get(threadId) || ''
      pendingContent.delete(threadId)

      const sid = store.threads[threadId]?.streamingMessageId
      if (sid) {
        _writeMsgs(threadId, prev => prev.map(m =>
          m.id === sid ? {
            ...m,
            content: [{ type: 'text', text: content }],
            metadata: event.elapsed_display || event.completed_at ? {
              custom: {
                ...(m.metadata?.custom || {}),
                ...(event.elapsed_seconds != null ? { elapsed_seconds: event.elapsed_seconds } : {}),
                ...(event.elapsed_display ? { elapsed_display: event.elapsed_display } : {}),
                ...(event.completed_at ? { completed_at: event.completed_at } : {}),
              },
            } : m.metadata,
          } : m
        ))
      }
      store.setThreadStreamingMessageId(threadId, null)
      store.setThreadStreamDone(threadId)
      setTimeout(() => store.setThreadStepStatuses(threadId, {}), 3000)

      // 流结束后从后端重新加载权威数据
      if (sid) {
        const fallbackMessages = [...(store.threads[threadId]?.messages ?? [])]
        reloadFromBackend(threadId, sid, fallbackMessages)

        // 刷新对话标题
        try {
          import('./api').then(({ getConversations }) => {
            getConversations().then(conversations => {
              const updatedConv = conversations.find(c => String(c.id) === threadId)
              if (updatedConv?.title) {
                window.dispatchEvent(new CustomEvent('thread-title-updated', {
                  detail: { threadId, title: updatedConv.title },
                }))
              }
            })
          })
        } catch {}
      }
      break
    }

    case 'snapshot': {
      const content = event.content || ''
      pendingContent.set(threadId, content)
      const sid = event.streaming_message_id || `ms-${Date.now()}`
      store.setThreadStreamingMessageId(threadId, sid)
      store.setThreadStepStatuses(threadId, event.step_statuses || {})

      if (event.is_queued) {
        store.setThreadQueueState(threadId, true, event.queue_position ?? 1)
      }

      const attachments = event.attachments || []
      _writeMsgs(threadId, prev => {
        const hasAssistant = prev.some(m => m.id === sid)
        if (hasAssistant) {
          return prev.map(m =>
            m.id === sid
              ? { ...m, content: [{ type: 'text', text: content }], metadata: attachments.length > 0 ? { custom: { attachments } } : m.metadata }
              : m
          )
        }
        return [...prev, {
          role: 'assistant', content: [{ type: 'text', text: content }],
          id: sid, createdAt: new Date(),
          metadata: attachments.length > 0 ? { custom: { attachments } } : undefined,
        }]
      })
      break
    }

    case 'idle':
      // 对话空闲，无需处理
      break

    case 'error':
      console.error('WS error:', event.message)
      store.setThreadStreamDone(threadId)
      break
  }
}

// ========== 文件上传 ==========

async function uploadFiles(files, tid) {
  const MAX_SIZE = 30 * 1024 * 1024
  const uploaded = []
  for (let i = 0; i < files.length; i++) {
    const attachment = files[i]
    if (attachment.file?.size > MAX_SIZE) {
      throw new Error(`文件过大: ${attachment.name || attachment.file?.name} (最大 30MB)`)
    }
    const formData = new FormData()
    formData.append('file', attachment.file)
    if (tid) formData.append('conversation_id', tid)

    const response = await fetch('/api/chat/upload', {
      method: 'POST',
      headers: getAuthHeaders(),
      body: formData,
    })
    if (!response.ok) throw new Error(`上传失败: ${attachment.name}`)
    const result = await response.json()
    uploaded.push({
      file_id: result.file_id,
      file_name: result.file_name,
      file_size: result.file_size,
      content_type: result.content_type,
      path: result.path,
    })
  }
  return uploaded
}

// ========== 后端消息转换 ==========

function convertBackendMessages(backendMessages) {
  return backendMessages.map(msg => {
    // agentlite sends naive isoformat (assume UTC); actforce sends RFC3339
    // with timezone. Parse both: only append 'Z' when no timezone present.
    const parseDate = (v) => {
      if (!v) return null
      const hasTz = /[zZ]|[+-]\d{2}:?\d{2}$/.test(v)
      const d = new Date(hasTz ? v : v + 'Z')
      return Number.isNaN(d.getTime()) ? null : d
    }
    const msgDate = parseDate(msg.created_at)
    const msgDateStr = msgDate ? msgDate.toISOString() : null
    const baseMetadata = msg.metadata ? { custom: msg.metadata } : undefined
    if (msg.role === 'assistant' && msg.created_at) {
      return {
        role: msg.role,
        content: [{ type: 'text', text: msg.content }],
        id: String(msg.id),
        createdAt: msgDate,
        metadata: { custom: { ...(baseMetadata?.custom || {}), completed_at: msgDateStr || msg.created_at } },
      }
    }
    return {
      role: msg.role,
      content: [{ type: 'text', text: msg.content }],
      id: String(msg.id),
      createdAt: msgDate,
      metadata: baseMetadata,
    }
  })
}

// ========== 流结束后从后端重新加载权威数据 ==========

async function reloadFromBackend(threadId, assistantId, fallbackMessages) {
  for (let attempt = 0; attempt <= 3; attempt++) {
    try {
      const backendMessages = await getThreadMessages(threadId)
      const assistantMsg = backendMessages.find(m => String(m.id) === assistantId)
      if (assistantMsg?.content) {
        _s().setThreadMessages(threadId, convertBackendMessages(backendMessages))
        return
      }
    } catch {}
    if (attempt < 3) await new Promise(r => setTimeout(r, 300 * Math.pow(2, attempt)))
  }
  _s().setThreadMessages(threadId, fallbackMessages)
}

export const useThreadStore = create((set, get) => ({
  threads: {},

  getThread: (threadId) => get().threads[threadId] || null,

  initThread: (threadId) => {
    if (!threadId) return
    const t = get().threads[threadId]
    // 只有正在运行且已初始化的线程才跳过（依赖 WS 实时更新）
    // 已完成的线程总是重新加载（处理多设备场景）
    if (t?.isRunning && t?.initialized && sharedWS?.readyState === WebSocket.OPEN) return
    const prev = initControllers.get(threadId)
    if (prev) prev.abort()
    const controller = new AbortController()
    initControllers.set(threadId, controller)
    _doInit(threadId, controller)
  },

  setThreadMessages: (threadId, messages) =>
    set((s) => {
      const t = s.threads[threadId]
      const current = t?.messages ?? []
      const next = typeof messages === 'function' ? messages(current) : messages
      return { threads: { ...s.threads, [threadId]: { ...(t || defaultThread), messages: next } } }
    }),

  setThreadRunning: (threadId, isRunning, startedAt) =>
    set((s) => {
      const t = s.threads[threadId]
      const resolvedStartedAt = startedAt ?? (isRunning && t?.startedAt ? t.startedAt : Date.now())
      if (!t) {
        if (!isRunning) return s
        return {
          threads: {
            ...s.threads,
            [threadId]: { ...defaultThread, isRunning: true, startedAt: resolvedStartedAt },
          },
        }
      }
      return {
        threads: {
          ...s.threads,
          [threadId]: {
            ...t,
            isRunning,
            startedAt: isRunning ? resolvedStartedAt : t.startedAt,
          },
        },
      }
    }),

  setThreadStreamDone: (threadId) =>
    set((s) => {
      const t = s.threads[threadId]
      if (!t) return s
      return {
        threads: {
          ...s.threads,
          [threadId]: { ...t, isRunning: false, isQueued: false, queuePosition: 0 },
        },
      }
    }),

  setThreadStepStatuses: (threadId, stepStatuses) =>
    set((s) => {
      const t = s.threads[threadId]
      if (!t) return s
      return { threads: { ...s.threads, [threadId]: { ...t, stepStatuses } } }
    }),

  setThreadStreamingMessageId: (threadId, streamingMessageId) =>
    set((s) => {
      const t = s.threads[threadId]
      if (!t) return s
      return { threads: { ...s.threads, [threadId]: { ...t, streamingMessageId } } }
    }),

  setThreadQueueState: (threadId, isQueued, queuePosition) =>
    set((s) => {
      const t = s.threads[threadId]
      if (!t) return s
      return { threads: { ...s.threads, [threadId]: { ...t, isQueued, queuePosition } } }
    }),

  setThreadLoading: (threadId, isLoading) =>
    set((s) => {
      const t = s.threads[threadId]
      return { threads: { ...s.threads, [threadId]: { ...(t || defaultThread), isLoading } } }
    }),

  setThreadInitialized: (threadId, initialized) =>
    set((s) => {
      const t = s.threads[threadId]
      return { threads: { ...s.threads, [threadId]: { ...(t || defaultThread), initialized } } }
    }),

  clearThread: (threadId) =>
    set((s) => {
      const { [threadId]: _, ...rest } = s.threads
      return { threads: rest }
    }),

  getRunningThreadIds: () => {
    const { threads } = get()
    return Object.keys(threads).filter((id) => threads[id].isRunning)
  },

  // ChatThread unmount 时调用 — 仅清理 init controller，WS 和状态保持
  abortThread: (threadId) => {
    const prev = initControllers.get(threadId)
    if (prev) prev.abort()
    initControllers.delete(threadId)
  },

  // ========== 发送消息 ==========
  sendMessage: async (threadId, { text, identityId, attachments, onThreadCreated, onClearAttach, onFillToInput, inferenceReady }) => {
    if (!text) return

    const store = get()
    store.setThreadStepStatuses(threadId, {})

    // 快速检查推理引擎就绪状态
    if (!inferenceReady) {
      try {
        const health = await getInferenceHealth()
        if (!health.is_ready && !health.ready) {
          store.setThreadMessages(threadId, [
            ...(store.threads[threadId]?.messages ?? []),
            { id: `error-${Date.now()}`, role: 'assistant', content: [{ type: 'text', text: '推理引擎尚未就绪，请稍后重试' }] },
          ])
          return
        }
      } catch {}
    }

    let currentThreadId = threadId

    // 新会话：创建 conversation
    if (!currentThreadId) {
      store.setThreadRunning(threadId || 'pending', true)
      try {
        const newConversation = await createConversation()
        currentThreadId = String(newConversation.id)
        onThreadCreated?.({
          id: currentThreadId,
          title: newConversation.title || '新对话',
          createdAt: newConversation.created_at,
          updatedAt: newConversation.updated_at,
        })
      } catch (e) {
        console.error('Failed to create conversation:', e)
        const errorMsg = e.message === 'No identity found'
          ? '需要登录后才能发送消息'
          : '创建对话失败，请重试'
        store.setThreadMessages(currentThreadId || 'pending', [
          { id: `error-${Date.now()}`, role: 'assistant', content: [{ type: 'text', text: errorMsg }] },
        ])
        store.setThreadStreamDone(currentThreadId || 'pending')
        return
      }
    }

    store.setThreadRunning(currentThreadId, true)

    // 上传附件
    let uploadedFiles = []
    if (attachments?.length > 0) {
      try {
        uploadedFiles = await uploadFiles(attachments, currentThreadId)
        onClearAttach?.()
      } catch (error) {
        console.error('文件上传失败:', error)
        onFillToInput?.(text)
        store.setThreadStreamDone(currentThreadId)
        return
      }
    }

    // 确保共享 WebSocket 已连接
    if (sharedWS?.readyState !== WebSocket.OPEN) {
      openSharedWS(identityId)
      try {
        await awaitSharedWSOpen()
      } catch (e) {
        console.error('WebSocket connection failed:', e)
        store.setThreadStreamDone(currentThreadId)
        return
      }
    }

    // 发送消息 over shared WebSocket
    sendSharedWS({
      type: 'send',
      message: text,
      conversation_id: currentThreadId,
      attachments: uploadedFiles.length > 0 ? uploadedFiles : undefined,
    })
    
    // 标记为已初始化（避免后续重复调用 _doInit）
    store.setThreadInitialized(currentThreadId, true)
  },
}))

/** 只订阅 isRunning 状态的便捷 hook */
export const useThreadIsRunning = (threadId) =>
  useThreadStore((s) => s.threads[threadId]?.isRunning ?? false)

/** 只订阅 isLoading 状态的便捷 hook */
export const useThreadIsLoading = (threadId) =>
  useThreadStore((s) => s.threads[threadId]?.isLoading ?? false)

// ========== 内部 ==========

const _s = () => useThreadStore.getState()

// ========== 初始化逻辑（历史加载 + WS 重连） ==========

async function _doInit(threadId, controller) {
  const cancelled = () => controller.signal.aborted

  try {
    const resp = await fetch(`/api/chat/conversations/${threadId}/running`, {
      headers: getAuthHeaders(),
    })
    const data = await resp.json()
    if (cancelled()) return

    // 如果未初始化，或者是已完成的线程，加载消息（处理多设备场景）
    const thread = _s().threads[threadId]
    if (!thread?.initialized || !data.running) {
      const isFirstVisit = !thread?.initialized
      if (isFirstVisit) _s().setThreadLoading(threadId, true)
      try {
        const backendMessages = await getThreadMessages(threadId)
        if (cancelled()) return
        _s().setThreadMessages(threadId, convertBackendMessages(backendMessages))
      } catch (error) {
        console.error('Failed to load messages:', error)
        if (!cancelled() && isFirstVisit) _s().setThreadMessages(threadId, [])
      } finally {
        if (!cancelled() && isFirstVisit) _s().setThreadLoading(threadId, false)
      }
    }

    if (!data.running) {
      _s().setThreadStreamDone(threadId)
      _s().setThreadInitialized(threadId, true)
      return
    }

    // ---- 任务正在运行 / 排队中：打开共享 WS 并同步 ----
    const backendStartedAt = data.started_at ? Math.floor(data.started_at * 1000) : null
    _s().setThreadRunning(threadId, true, backendStartedAt)
    _s().setThreadLoading(threadId, false)

    if (data.is_queued) {
      _s().setThreadQueueState(threadId, true, data.queue_position ?? 1)
    }

    const identityId = getStoredIdentity() || ''
    openSharedWS(identityId)
    try {
      await awaitSharedWSOpen()
    } catch (e) {
      console.error('WebSocket connection failed for sync:', e)
      _s().setThreadStreamDone(threadId)
      return
    }

    // 发送 sync 消息获取当前状态
    sendSharedWS({ type: 'sync', conversation_id: threadId })
    
    // 标记为已初始化
    _s().setThreadInitialized(threadId, true)
  } catch (error) {
    console.error('Failed to init thread:', error)
    if (cancelled()) return
    const thread = _s().threads[threadId]
    if (!thread?.isRunning && !thread?.messages?.length) {
      _s().setThreadLoading(threadId, true)
      try {
        const backendMessages = await getThreadMessages(threadId)
        if (cancelled()) return
        _s().setThreadMessages(threadId, convertBackendMessages(backendMessages))
      } catch (loadError) {
        console.error('Failed to load messages:', loadError)
        if (!cancelled()) _s().setThreadMessages(threadId, [])
      } finally {
        if (!cancelled()) _s().setThreadLoading(threadId, false)
      }
    }
    // 即使出错，也标记为已初始化（避免重复调用）
    _s().setThreadInitialized(threadId, true)
  }
}
