import { useState, useEffect, useCallback } from 'react'
import { Plus, MessageSquare, Trash2, PanelLeftClose, PanelLeft, Pencil, Check, X } from 'lucide-react'
import { getConversations, createConversation, deleteThreadMessages, updateConversationTitle } from '../lib/api'
import { useAppConfig } from '../lib/AppConfigContext'
import { useThreadIsRunning, useThreadStore, openSharedWS, sendSharedWS, awaitSharedWSOpen } from '../lib/threadStore'
import { getStoredIdentity } from '../lib/auth'

const STORAGE_KEY = 'actforce-threads'

// 从 localStorage 加载 threads
function loadThreads() {
  try {
    const data = localStorage.getItem(STORAGE_KEY)
    return data ? JSON.parse(data) : []
  } catch {
    return []
  }
}

// 保存 threads 到 localStorage
function saveThreads(threads) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(threads))
}

// 流式状态指示灯（独立从 store 订阅，避免父组件传参）
const StreamingDot = ({ threadId }) => {
  const isRunning = useThreadIsRunning(threadId)
  if (!isRunning) return null
  return (
    <span className="flex gap-0.5">
      <span className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse" style={{ animationDelay: '0ms' }} />
      <span className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse" style={{ animationDelay: '150ms' }} />
      <span className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse" style={{ animationDelay: '300ms' }} />
    </span>
  )
}

export function ThreadSidebar({ 
  threads, 
  activeThreadId, 
  onSelectThread, 
  onCreateThread,
  onDeleteThread,
  collapsed,
  onToggleCollapse,
}) {
  const { productName, version, releaseDate, ui } = useAppConfig()
  const [editingThreadId, setEditingThreadId] = useState(null)
  const [editingTitle, setEditingTitle] = useState('')

  // 使用后端配置的 logo_url 或默认 logo
  const logoUrl = ui?.logo_url || '/actforce-logo.svg'

  const handleStartEdit = (e, thread) => {
    e.stopPropagation()
    setEditingThreadId(thread.id)
    setEditingTitle(thread.title || '新对话')
  }

  const handleCancelEdit = () => {
    setEditingThreadId(null)
    setEditingTitle('')
  }

  const handleSaveEdit = async (threadId) => {
    if (!editingTitle.trim()) {
      handleCancelEdit()
      return
    }
    
    try {
      await updateConversationTitle(threadId, editingTitle.trim())
      // 触发标题更新事件
      window.dispatchEvent(new CustomEvent('thread-title-updated', { 
        detail: { threadId, title: editingTitle.trim() } 
      }))
      setEditingThreadId(null)
      setEditingTitle('')
    } catch (error) {
      console.error('Failed to update title:', error)
    }
  }

  const handleKeyDown = (e, threadId) => {
    if (e.key === 'Enter') {
      handleSaveEdit(threadId)
    } else if (e.key === 'Escape') {
      handleCancelEdit()
    }
  }

  return (
    <aside 
      className={`
        flex flex-col border-r border-border bg-card 
        transition-all duration-300 ease-in-out
        ${collapsed ? 'w-0 overflow-hidden border-r-0' : 'w-72'}
      `}
    >
      {/* Header - 显示产品 Logo 和名称 */}
      <div className="p-4 border-b border-border">
        <div className="flex items-center gap-2">
          <img 
            src={logoUrl} 
            alt={productName} 
            className="w-6 h-6 object-contain"
            onError={(e) => {
              // 如果 logo 加载失败，隐藏图片
              e.target.style.display = 'none'
            }}
          />
          <span className="font-bold text-lg text-foreground truncate">{productName}</span>
        </div>
      </div>

      {/* 新建对话按钮 */}
      <div className="p-3">
        <button
          onClick={onCreateThread}
          className="w-full flex items-center gap-2 px-3 py-2 rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition-colors text-sm font-medium"
        >
          <Plus className="h-4 w-4" />
          <span>新对话</span>
        </button>
      </div>

      {/* Thread 列表 */}
      <div className="flex-1 overflow-y-auto px-3 pb-3 space-y-1">
        {threads.length === 0 ? (
          <div className="text-center py-8 text-muted-foreground text-sm">
            <MessageSquare className="h-8 w-8 mx-auto mb-2 opacity-50" />
            <p>暂无对话</p>
          </div>
        ) : (
          threads.map((thread) => {
            // 格式化时间显示
            const formatDate = (dateString) => {
              if (!dateString) return ''
              // agentlite sends naive isoformat ("2026-09-06T10:00:00.123456",
              // interpreted as UTC) while actforce sends RFC3339 with a 'Z'
              // suffix. Append 'Z' only when the string has no timezone, so an
              // already-zoned timestamp doesn't become "...ZZ" (Invalid Date).
              const hasTz = /[zZ]|[+-]\d{2}:?\d{2}$/.test(dateString)
              const date = new Date(hasTz ? dateString : dateString + 'Z')
              if (Number.isNaN(date.getTime())) return '' // 解析失败不显示
              const now = new Date()
              
              // 使用本地时间的日期部分进行比较
              const today = new Date(now.getFullYear(), now.getMonth(), now.getDate())
              const dateLocal = new Date(date.getFullYear(), date.getMonth(), date.getDate())
              const diffDays = Math.floor((today - dateLocal) / (1000 * 60 * 60 * 24))
              
              if (diffDays === 0) {
                return '今天'
              } else if (diffDays === 1) {
                return '昨天'
              } else if (diffDays < 7) {
                return `${diffDays}天前`
              } else {
                return date.toLocaleDateString('zh-CN', { 
                  month: 'short', 
                  day: 'numeric' 
                })
              }
            }
            
            const timeText = thread.updatedAt 
              ? formatDate(thread.updatedAt)
              : thread.createdAt 
                ? formatDate(thread.createdAt)
                : ''

            return (
              <div
                key={thread.id}
                className={`
                  group flex flex-col gap-1 pl-3 pr-3 py-2 rounded-r-lg cursor-pointer transition-colors text-sm border-l-2
                  ${thread.id === activeThreadId 
                    ? 'bg-primary/10 text-foreground border-l-primary font-medium' 
                    : 'border-l-transparent hover:bg-muted/50 text-muted-foreground hover:text-foreground'}
                `}
                onClick={() => onSelectThread(thread.id)}
              >
                <div className="flex items-center gap-2 min-w-0">
                  <MessageSquare className="h-4 w-4 flex-shrink-0" />
                  <StreamingDot threadId={thread.id} />
                  
                  {editingThreadId === thread.id ? (
                    // 编辑模式
                    <div className="flex-1 min-w-0 flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
                      <input
                        type="text"
                        value={editingTitle}
                        onChange={(e) => setEditingTitle(e.target.value)}
                        onKeyDown={(e) => handleKeyDown(e, thread.id)}
                        onBlur={() => handleSaveEdit(thread.id)}
                        className="flex-1 min-w-0 px-2 py-1 text-sm bg-background border border-border rounded focus:outline-none focus:ring-1 focus:ring-primary"
                        autoFocus
                        maxLength={50}
                      />
                      <button
                        onMouseDown={(e) => e.preventDefault()}
                        onClick={() => handleSaveEdit(thread.id)}
                        className="h-6 w-6 flex-shrink-0 rounded hover:bg-primary/10 hover:text-primary transition-all flex items-center justify-center"
                        title="保存"
                      >
                        <Check className="h-3 w-3" />
                      </button>
                      <button
                        onMouseDown={(e) => e.preventDefault()}
                        onClick={handleCancelEdit}
                        className="h-6 w-6 flex-shrink-0 rounded hover:bg-muted transition-all flex items-center justify-center"
                        title="取消"
                      >
                        <X className="h-3 w-3" />
                      </button>
                    </div>
                  ) : (
                    // 显示模式
                    <>
                      <span className="flex-1 truncate">
                        {thread.title || '新对话'}
                      </span>
                      <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                        <button
                          onClick={(e) => handleStartEdit(e, thread)}
                          className="h-6 w-6 rounded hover:bg-muted transition-all flex items-center justify-center"
                          title="重命名"
                        >
                          <Pencil className="h-3 w-3" />
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation()
                            onDeleteThread(thread.id)
                          }}
                          className="h-6 w-6 rounded hover:bg-destructive/10 hover:text-destructive transition-all flex items-center justify-center"
                          title="删除对话"
                        >
                          <Trash2 className="h-3 w-3" />
                        </button>
                      </div>
                    </>
                  )}
                </div>
                
                {/* 时间显示 / 编辑提示 */}
                {editingThreadId === thread.id ? (
                  <div className="text-[10px] text-muted-foreground ml-6">
                    Enter 保存 · Esc 取消 · 点击外部自动保存
                  </div>
                ) : timeText ? (
                  <div className="text-xs text-muted-foreground ml-6">
                    {timeText}
                  </div>
                ) : null}
              </div>
            )
          })
        )}
      </div>

      {/* Footer */}
      <div className="p-3 border-t border-border text-xs text-muted-foreground text-center">
        {productName} v{version} {releaseDate ? `(${releaseDate})` : ''}
      </div>
    </aside>
  )
}

// 展开按钮（当 sidebar 收起时显示）
export function SidebarToggle({ onClick }) {
  return (
    <button
      onClick={onClick}
      className="h-5 w-5 rounded-md hover:bg-muted transition-colors flex items-center justify-center"
      title="展开侧边栏"
    >
      <PanelLeft className="h-5 w-5" />
    </button>
  )
}

// Thread 管理 Hook
export function useThreadManager(identityId) {
  const [threads, setThreads] = useState([])
  const [activeThreadId, setActiveThreadId] = useState(null)
  const [collapsed, setCollapsed] = useState(false)
  const [isLoading, setIsLoading] = useState(true)

  // 从后端加载对话列表（identityId 变化时重新加载，如登录后）
  useEffect(() => {
    setActiveThreadId(null)  // 清除当前对话视图
    loadThreads()
    
    // 监听标题更新事件
    const handleTitleUpdate = (event) => {
      const { threadId, title } = event.detail
      setThreads(prev => 
        prev.map(t => t.id === threadId ? { ...t, title } : t)
      )
    }
    
    window.addEventListener('thread-title-updated', handleTitleUpdate)
    return () => {
      window.removeEventListener('thread-title-updated', handleTitleUpdate)
    }
  }, [identityId])

  const loadThreads = async () => {
    try {
      const conversations = await getConversations()
      const mappedThreads = conversations.map(conv => ({
        id: String(conv.id),
        title: conv.title || '新对话',
        createdAt: conv.created_at,
        updatedAt: conv.updated_at,
      }))
      setThreads(mappedThreads)

      // 对正在运行的会话：打开 WS 并 sync，将状态写入 store
      const runningConvs = conversations.filter(c => c.running)
      if (runningConvs.length > 0) {
        const identityId = getStoredIdentity() || ''
        openSharedWS(identityId)
        try {
          await awaitSharedWSOpen()
          for (const conv of runningConvs) {
            const tid = String(conv.id)
            useThreadStore.getState().setThreadRunning(tid, true, null)
            sendSharedWS({ type: 'sync', conversation_id: tid })
          }
        } catch (e) {
          console.error('WS sync on page load failed:', e)
        }
      }
      
      // 不自动选中任何对话，显示欢迎页
      // 用户主动点击历史对话或输入消息时才创建/选中对话
    } catch (error) {
      console.error('Failed to load conversations:', error)
    } finally {
      setIsLoading(false)
    }
  }

  // 创建新 thread
  const createThread = async () => {
    try {
      const newConversation = await createConversation()
      const newThread = {
        id: String(newConversation.id),
        title: newConversation.title || '新对话',
        createdAt: newConversation.created_at,
        updatedAt: newConversation.updated_at,
      }
      setThreads(prev => [newThread, ...prev])
      setActiveThreadId(newThread.id)
      return newThread.id
    } catch (error) {
      console.error('Failed to create conversation:', error)
      return null
    }
  }

  // 删除 thread
  const deleteThread = async (threadId) => {
    try {
      await deleteThreadMessages(threadId)
      
      setThreads(prev => {
        const newThreads = prev.filter(t => t.id !== threadId)
        
        // 如果删除的是当前 thread，不自动切换，显示欢迎页
        if (threadId === activeThreadId) {
          setActiveThreadId(null)
        }
        
        return newThreads
      })
    } catch (error) {
      console.error('Failed to delete conversation:', error)
    }
  }

  // 更新 thread 标题
  const updateThreadTitle = (threadId, title) => {
    setThreads(prev => 
      prev.map(t => t.id === threadId ? { ...t, title } : t)
    )
  }

  // 添加 thread（由 ChatThread 创建对话后回调）
  const addThread = (thread) => {
    setThreads(prev => [thread, ...prev])
  }

  // 切换 sidebar 折叠状态
  const toggleCollapse = () => {
    setCollapsed(prev => !prev)
  }

  return {
    threads,
    activeThreadId,
    collapsed,
    isLoading,
    setActiveThreadId,
    createThread,
    addThread,
    deleteThread,
    updateThreadTitle,
    toggleCollapse,
  }
}
