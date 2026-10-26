import { useState, useCallback, useMemo, createContext, useContext, lazy, Suspense } from 'react'
import { ShieldAlert, AlertTriangle, FolderOpen } from 'lucide-react'
import { ChatThread } from './components/Thread'
import { ThreadSidebar, SidebarToggle, useThreadManager } from './components/ThreadSidebar'
import { LoginDialog } from './components/LoginDialog'
import { UserMenu } from './components/UserMenu'
import { AuthProvider, useAuth } from './lib/AuthContext'
import { AppConfigProvider, useAppConfig } from './lib/AppConfigContext'
import { EntitlementsProvider, useEntitlements } from './lib/EntitlementsContext'
import { ToastContainer } from './components/Toast'
// Lazy-load the file manager page so its code (file-manager components,
// office/audio preview deps) becomes a separate chunk, loaded only when
// the user opens file management — keeps the chat first screen lean.
const FileManagerPage = lazy(() =>
  import('./components/FileManagerPage').then(m => ({ default: m.FileManagerPage }))
);
import { useInferenceHealth } from './hooks/useInferenceHealth'

// Shared inference health context — a single poll at App level, consumed by ChatThread
const InferenceHealthContext = createContext({ isReady: true, processState: 'stopped', message: '' })
export const useInferenceHealthContext = () => useContext(InferenceHealthContext)

function AppContent({ extensions = {} }) {
  const [showLoginDialog, setShowLoginDialog] = useState(false)
  // 视图切换：'chat' 聊天主界面 | 'files' 全屏文件管理页面（替代原 FileBrowserDialog 弹窗）
  const [activeView, setActiveView] = useState('chat')
  const { identityId, isLoggedIn, showSessionExpired, loginRequired, clearLoginRequired } = useAuth()
  const { isLoading: configLoading, error: configError, productName } = useAppConfig()
  const {
    adminTabs = [],
    banners = null,
    accessFallback = null,
    headerActions = null,
  } = extensions
  const { entitlements, isLoading: entitlementsLoading, isBlocked } = useEntitlements()

  // Single inference health poll at App level — avoids per-ChatThread duplicates
  const inferenceHealth = useInferenceHealth()

  const {
    threads,
    activeThreadId,
    collapsed,
    setActiveThreadId,
    createThread,
    addThread,
    deleteThread,
    toggleCollapse,
  } = useThreadManager(identityId)

  // 获取当前活跃聊天的标题
  const activeThreadTitle = useMemo(() => {
    if (!activeThreadId) return productName
    const thread = threads.find(t => t.id === activeThreadId)
    return thread?.title || productName
  }, [threads, activeThreadId, productName])

  // ChatThread 内部创建新对话时的回调
  const handleThreadCreated = useCallback((newThread) => {
    addThread(newThread)
    setActiveThreadId(newThread.id)
  }, [addThread, setActiveThreadId])

  // 带登录检查的创建对话
  const handleCreateThread = useCallback(async () => {
    if (!identityId) {
      setShowLoginDialog(true)
      return null
    }
    return await createThread()
  }, [identityId, createThread])

  // 授权状态由后端统一决定；前端只负责展示。
  const accessBlocked = !configError && isBlocked()

  // 文件管理页面只在"正常聊天视图"可用的情况下展示（保持现有守卫顺序）：
  // 配置正常 + 授权未阻断 + 已登录/允许匿名
  const canShowFiles = !configError && !accessBlocked && (isLoggedIn || identityId)

  // 打开全屏文件管理页面（视图切换，替代原弹窗）
  const handleOpenFileManagement = useCallback(() => {
    if (!canShowFiles) {
      // 未登录时引导登录；license 无效时停留在聊天视图的守卫界面
      if (!identityId && !isLoggedIn) {
        setShowLoginDialog(true)
      }
      return
    }
    setActiveView('files')
  }, [canShowFiles, identityId, isLoggedIn])

  // 配置加载中显示 loading
  if (configLoading || entitlementsLoading) {
    return (
      <InferenceHealthContext.Provider value={inferenceHealth}>
      <div className="h-screen w-full flex items-center justify-center bg-background">
        <div className="text-center">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary mx-auto mb-4"></div>
          <p className="text-muted-foreground">加载中...</p>
        </div>
      </div>
      </InferenceHealthContext.Provider>
    )
  }

  return (
      <InferenceHealthContext.Provider value={inferenceHealth}>
      <div className="h-screen w-full flex flex-col bg-background">
        {/* Toast 通知容器 */}
        <ToastContainer />

        {/* 产品级提示位（私有产品可注入许可证到期提醒等） */}
        {banners}

        {activeView === 'files' && canShowFiles ? (
          /* 全屏文件管理页面（懒加载独立 chunk，打开时才下载）*/
          <Suspense fallback={<div className="h-screen w-full flex items-center justify-center bg-background text-muted-foreground text-sm">文件管理加载中...</div>}>
            <FileManagerPage onBack={() => setActiveView('chat')} />
          </Suspense>
        ) : (
        <div className="flex-1 flex overflow-hidden">
          {/* 授权未阻断时显示侧边栏 */}
          {!accessBlocked && (
            <ThreadSidebar
              threads={threads}
              activeThreadId={activeThreadId}
              onSelectThread={setActiveThreadId}
              onCreateThread={handleCreateThread}
              onDeleteThread={deleteThread}
              collapsed={collapsed}
              onToggleCollapse={toggleCollapse}
            />
          )}

          {/* 主内容区域 */}
          <div className="flex-1 flex flex-col overflow-hidden">
            {/* Header */}
            <header className="bg-card px-4 py-2 flex items-center gap-3 relative">
              <SidebarToggle onClick={toggleCollapse} />
              <h1 className="text font-bold text-foreground flex-1 truncate">{activeThreadTitle}</h1>

              {/* 文件管理按钮 - 始终显示，点击时检查状态 */}
              <button
                onClick={handleOpenFileManagement}
                className="flex items-center gap-2 px-3 py-2 text-sm font-medium rounded-lg bg-muted hover:bg-muted/80 text-foreground transition-colors"
                title="打开文件管理"
              >
                <FolderOpen className="h-4 w-4" />
                <span className="hidden sm:inline">文件管理</span>
              </button>

              {headerActions}

              <UserMenu onLoginClick={() => setShowLoginDialog(true)} adminTabs={adminTabs} />

              {/* 会话过期提示 */}
              {showSessionExpired && (
                <div className="absolute top-full left-1/2 -translate-x-1/2 mt-2 px-4 py-2 bg-amber-500 text-white text-sm font-medium rounded-lg shadow-lg animate-fade-in z-50">
                  会话已过期,请重新登录
                </div>
              )}
            </header>

            {/* Login Dialog */}
            <LoginDialog
              isOpen={showLoginDialog || loginRequired}
              onClose={() => { setShowLoginDialog(false); clearLoginRequired() }}
            />

            {/* 聊天区域 - 仅 License 有效时显示 */}
            <div className="flex-1 overflow-hidden">
              {configError ? (
                /* 配置加载失败 - 显示服务器错误 */
                <div className="h-full w-full flex items-center justify-center p-6">
                  <div className="max-w-2xl w-full">
                    <div className="bg-card border-2 border-amber-500 rounded-lg shadow-xl p-8">
                      <div className="flex items-start gap-4 mb-6">
                        <div className="flex-shrink-0">
                          <div className="w-16 h-16 rounded-full bg-amber-500/10 flex items-center justify-center">
                            <AlertTriangle className="w-8 h-8 text-amber-500" />
                          </div>
                        </div>
                        <div className="flex-1">
                          <h2 className="text-2xl font-bold text-foreground mb-2">服务器连接异常</h2>
                          <p className="text-muted-foreground mb-4">
                            无法连接到后端服务，请检查：
                          </p>
                          <div className="bg-muted rounded-lg p-4 mb-4">
                            <ul className="list-disc list-inside space-y-1 text-sm text-foreground/80">
                              <li>后端服务是否正在运行</li>
                              <li>网络连接是否正常</li>
                              <li>API 地址配置是否正确</li>
                            </ul>
                          </div>
                          {configError && (
                            <div className="bg-destructive/10 border border-destructive/30 rounded-lg p-3">
                              <p className="text-sm text-destructive font-mono">
                                错误信息: {configError}
                              </p>
                            </div>
                          )}
                          <div className="mt-4 flex gap-2">
                            <button
                              onClick={() => window.location.reload()}
                              className="px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors"
                            >
                              重新加载
                            </button>
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              ) : !configLoading && !identityId && !isLoggedIn ? (
                /* 匿名已禁用且未登录 → 提示登录 */
                <div className="h-full flex items-center justify-center p-6">
                  <div className="text-center">
                    <ShieldAlert className="w-16 h-16 text-muted-foreground mx-auto mb-4" />
                    <h2 className="text-xl font-semibold text-foreground mb-2">需要登录</h2>
                    <p className="text-muted-foreground mb-6">匿名访问已禁用，请登录后使用</p>
                    <button
                      onClick={() => setShowLoginDialog(true)}
                      className="px-6 py-2.5 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors font-medium"
                    >
                      登录
                    </button>
                  </div>
                </div>
              ) : accessBlocked ? (
                accessFallback || (
                  <div className="h-full w-full flex items-center justify-center p-6">
                    <div className="max-w-2xl w-full text-center">
                      <ShieldAlert className="w-12 h-12 text-muted-foreground mx-auto mb-4" />
                      <h2 className="text-xl font-semibold text-foreground mb-2">服务暂不可用</h2>
                      <p className="text-muted-foreground">
                        {entitlements?.messages?.join('；') || '当前授权状态不允许访问，请联系管理员。'}
                      </p>
                      <button
                        onClick={() => window.location.reload()}
                        className="mt-6 px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 transition-colors"
                      >
                        重新加载
                      </button>
                    </div>
                  </div>
                )
              ) : (
                <div className="flex flex-col h-full">
                  {/* 聊天区域 */}
                  <div className="flex-1 overflow-hidden">
                    <ChatThread
                      key={activeThreadId || 'empty'}
                      threadId={activeThreadId}
                      onThreadCreated={handleThreadCreated}
                    />
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
        )}
      </div>
      </InferenceHealthContext.Provider>
  )
}

function App({ extensions }) {
  return (
    <AppConfigProvider>
      <AuthProvider>
        <EntitlementsProvider failOpen={extensions?.entitlementsFailOpen ?? true}>
          <AppContent extensions={extensions} />
        </EntitlementsProvider>
      </AuthProvider>
    </AppConfigProvider>
  )
}

export default App
