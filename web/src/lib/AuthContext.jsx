import { createContext, useContext, useState, useEffect, useCallback } from 'react'
import {
  initIdentity,
  login as apiLogin,
  logout as apiLogout,
  getCurrentUser,
  getStoredIdentity,
  getStoredUser,
  getStoredToken,
  isLoggedIn as checkIsLoggedIn,
  getAuthHeaders,
  clearAuthStorage,
  handleApiResponse,
} from './auth'

const AuthContext = createContext(null)

export function AuthProvider({ children }) {
  const [user, setUser] = useState(getStoredUser())
  const [identityId, setIdentityId] = useState(getStoredIdentity())
  const [isLoading, setIsLoading] = useState(true)
  const [error, setError] = useState(null)
  const [showSessionExpired, setShowSessionExpired] = useState(false)
  const [loginRequired, setLoginRequired] = useState(false)

  // 处理 401 未授权错误
  const handleUnauthorized = useCallback(() => {
    // 清除认证信息
    clearAuthStorage()
    setUser(null)
    // 显示会话过期提示
    setShowSessionExpired(true)
    // 3秒后自动隐藏提示
    setTimeout(() => setShowSessionExpired(false), 3000)
  }, [])

  // Initialize identity on mount
  useEffect(() => {
    const init = async () => {
      try {
        setIsLoading(true)
        setError(null)
        
        // Initialize or restore identity
        const result = await initIdentity()
        setIdentityId(result.identity_id)
        
        // If we have a token, try to get current user
        if (getStoredToken()) {
          try {
            const userResult = await getCurrentUser()
            if (userResult?.user) {
              setUser(userResult.user)
            } else {
              // Token expired or invalid
              handleUnauthorized()
            }
          } catch (e) {
            // Token might be expired
            console.warn('Failed to restore user session:', e)
            handleUnauthorized()
          }
        }
      } catch (e) {
        console.error('Failed to initialize auth:', e)
        setError(e.message)
      } finally {
        setIsLoading(false)
      }
    }
    
    init()
  }, [handleUnauthorized])

  const login = useCallback(async (loginId, password) => {
    try {
      setIsLoading(true)
      setError(null)
      setLoginRequired(false)
      
      const result = await apiLogin(loginId, password)
      setUser(result.user)
      if (result.identity_id) {
        setIdentityId(result.identity_id)
      }
      
      return result
    } catch (e) {
      setError(e.message)
      throw e
    } finally {
      setIsLoading(false)
    }
  }, [])

  const logout = useCallback(async () => {
    try {
      setIsLoading(true)
      await apiLogout()
      setUser(null)
      // 重新初始化身份：匿名允许 → 新 identity；匿名禁止 → 弹出登录
      try {
        const result = await initIdentity()
        setIdentityId(result.identity_id)
      } catch {
        // 匿名被禁用，清除 identity 并触发登录对话框
        localStorage.removeItem('actforce-identity')
        setIdentityId(null)
        setLoginRequired(true)
      }
    } catch (e) {
      console.error('Logout error:', e)
    } finally {
      setIsLoading(false)
    }
  }, [])

  const refreshUser = useCallback(async () => {
    try {
      const result = await getCurrentUser()
      if (result?.user) {
        setUser(result.user)
      } else {
        handleUnauthorized()
      }
      return result
    } catch (e) {
      console.error('Failed to refresh user:', e)
      handleUnauthorized()
      return null
    }
  }, [handleUnauthorized])

  const value = {
    user,
    identityId,
    isLoading,
    error,
    isLoggedIn: checkIsLoggedIn(),
    showSessionExpired,
    loginRequired,
    clearLoginRequired: () => setLoginRequired(false),
    login,
    logout,
    refreshUser,
    handleUnauthorized,
    handleApiResponse,
    getAuthHeaders,
  }

  return (
    <AuthContext.Provider value={value}>
      {children}
    </AuthContext.Provider>
  )
}

export function useAuth() {
  const context = useContext(AuthContext)
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider')
  }
  return context
}

export default AuthContext
