import { useState } from 'react'
import { LogIn, X, Loader2, AlertCircle } from 'lucide-react'
import { useAuth } from '../lib/AuthContext'

/**
 * 登录对话框
 */
export function LoginDialog({ isOpen, onClose }) {
  const { login, isLoading } = useAuth()
  const [loginId, setLoginId] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')

  const handleSubmit = async (e) => {
    e.preventDefault()
    setError('')
    
    if (!loginId.trim() || !password.trim()) {
      setError('请输入用户名和密码')
      return
    }
    
    try {
      await login(loginId.trim(), password)
      onClose()
      setLoginId('')
      setPassword('')
    } catch (e) {
      setError(e.message || '登录失败')
    }
  }

  if (!isOpen) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div 
        className="absolute inset-0 bg-black/60"
        onClick={onClose}
      />
      
      {/* Dialog */}
      <div className="relative bg-card border border-border rounded-lg shadow-xl w-full max-w-md mx-4 p-6">
        {/* Close button */}
        <button
          onClick={onClose}
          className="absolute top-4 right-4 text-muted-foreground hover:text-foreground transition-colors"
        >
          <X className="h-5 w-5" />
        </button>
        
        {/* Header */}
        <div className="text-center mb-6">
          <div className="inline-flex items-center justify-center w-12 h-12 rounded-full bg-primary/10 mb-4">
            <LogIn className="h-6 w-6 text-primary" />
          </div>
          <h2 className="text-xl font-semibold text-foreground">登录</h2>
          <p className="text-sm text-muted-foreground mt-1">
            登录以同步您的对话历史
          </p>
        </div>
        
        {/* Form */}
        <form onSubmit={handleSubmit} className="space-y-4">
          {error && (
            <div className="flex items-center gap-2 p-3 rounded-lg bg-destructive/10 text-destructive text-sm">
              <AlertCircle className="h-4 w-4 flex-shrink-0" />
              <span>{error}</span>
            </div>
          )}
          
          <div>
            <label 
              htmlFor="loginId" 
              className="block text-sm font-medium text-foreground mb-1.5"
            >
              用户名
            </label>
            <input
              id="loginId"
              type="text"
              value={loginId}
              onChange={(e) => setLoginId(e.target.value)}
              className="w-full px-3 py-2 rounded-lg border border-input bg-background text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary focus:border-transparent transition-colors"
              placeholder="请输入用户名"
              disabled={isLoading}
              autoComplete="username"
            />
          </div>
          
          <div>
            <label 
              htmlFor="password" 
              className="block text-sm font-medium text-foreground mb-1.5"
            >
              密码
            </label>
            <input
              id="password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="w-full px-3 py-2 rounded-lg border border-input bg-background text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary focus:border-transparent transition-colors"
              placeholder="请输入密码"
              disabled={isLoading}
              autoComplete="current-password"
            />
          </div>
          
          <button
            type="submit"
            disabled={isLoading}
            className="w-full flex items-center justify-center gap-2 px-4 py-2.5 rounded-lg bg-primary text-primary-foreground font-medium hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {isLoading ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                <span>登录中...</span>
              </>
            ) : (
              <>
                <LogIn className="h-4 w-4" />
                <span>登录</span>
              </>
            )}
          </button>
        </form>
        
        {/* Footer */}
        <p className="text-center text-xs text-muted-foreground mt-6">
          未登录时，您的对话将以匿名身份保存
        </p>
      </div>
    </div>
  )
}
