import { useState } from 'react'
import { X, Loader2, AlertCircle, UserCircle, Key, Save, Check } from 'lucide-react'
import { useAuth } from '../lib/AuthContext'
import { UserAvatar } from './UserAvatar'
import { getAuthHeaders } from '../lib/auth'
import { md5 } from '../lib/auth'

/**
 * 个人设置对话框
 * - 修改昵称
 * - 修改密码
 */
export function ProfileSettingsDialog({ isOpen, onClose }) {
  const { user, refreshUser } = useAuth()
  const [displayName, setDisplayName] = useState(user?.display_name || '')
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState('')
  const [success, setSuccess] = useState('')
  const [activeTab, setActiveTab] = useState('profile') // 'profile' | 'password'

  const handleUpdateProfile = async (e) => {
    e.preventDefault()
    setError('')
    setSuccess('')
    setIsLoading(true)
    
    try {
      const response = await fetch('/api/auth/profile', {
        method: 'PUT',
        headers: { 
          'Content-Type': 'application/json',
          ...getAuthHeaders(),
        },
        body: JSON.stringify({ display_name: displayName }),
      })
      
      if (!response.ok) {
        const data = await response.json()
        throw new Error(data.detail || '更新失败')
      }
      
      await refreshUser()
      setSuccess('个人信息已更新')
    } catch (e) {
      setError(e.message)
    } finally {
      setIsLoading(false)
    }
  }

  const handleChangePassword = async (e) => {
    e.preventDefault()
    setError('')
    setSuccess('')
    
    if (newPassword !== confirmPassword) {
      setError('两次输入的密码不一致')
      return
    }
    
    if (newPassword.length < 6) {
      setError('新密码至少需要6个字符')
      return
    }
    
    setIsLoading(true)
    
    try {
      // MD5 hash passwords before sending to backend
      const currentPasswordMd5 = md5(currentPassword)
      const newPasswordMd5 = md5(newPassword)
      
      const response = await fetch('/api/auth/change-password', {
        method: 'POST',
        headers: { 
          'Content-Type': 'application/json',
          ...getAuthHeaders(),
        },
        body: JSON.stringify({ 
          current_password: currentPasswordMd5,
          new_password: newPasswordMd5,
        }),
      })
      
      if (!response.ok) {
        const data = await response.json()
        throw new Error(data.detail || '修改密码失败')
      }
      
      setSuccess('密码已修改')
      setCurrentPassword('')
      setNewPassword('')
      setConfirmPassword('')
    } catch (e) {
      setError(e.message)
    } finally {
      setIsLoading(false)
    }
  }

  const handleTabChange = (tab) => {
    setActiveTab(tab)
    setError('')
    setSuccess('')
  }

  if (!isOpen) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/60" onClick={onClose} />
      
      <div className="relative bg-card border border-border rounded-lg shadow-xl w-full max-w-md mx-4 min-h-[560px] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-border">
          <h2 className="text-lg font-semibold text-foreground">个人设置</h2>
          <button onClick={onClose} className="text-muted-foreground hover:text-foreground">
            <X className="h-5 w-5" />
          </button>
        </div>
        
        {/* User Info */}
        <div className="px-6 py-4 border-b border-border flex items-center gap-4">
          <UserAvatar user={user} size="xl" />
          <div>
            <p className="font-medium text-foreground">{user?.display_name || user?.login_id}</p>
            <p className="text-sm text-muted-foreground">{user?.login_id}</p>
            <span className={`inline-block mt-1 px-2 py-0.5 rounded text-xs ${user?.role === 'admin' ? 'bg-amber-500/20 text-amber-600' : 'bg-muted text-muted-foreground'}`}>
              {user?.role === 'admin' ? '管理员' : '普通用户'}
            </span>
          </div>
        </div>
        
        {/* Tabs */}
        <div className="flex border-b border-border">
          <button
            onClick={() => handleTabChange('profile')}
            className={`flex-1 px-4 py-3 text-sm font-medium transition-colors ${activeTab === 'profile' ? 'text-primary border-b-2 border-primary' : 'text-muted-foreground hover:text-foreground'}`}
          >
            <UserCircle className="h-4 w-4 inline mr-2" />
            个人信息
          </button>
          <button
            onClick={() => handleTabChange('password')}
            className={`flex-1 px-4 py-3 text-sm font-medium transition-colors ${activeTab === 'password' ? 'text-primary border-b-2 border-primary' : 'text-muted-foreground hover:text-foreground'}`}
          >
            <Key className="h-4 w-4 inline mr-2" />
            修改密码
          </button>
        </div>
        
        {/* Content */}
        <div className="p-6 flex-1">
          {(error || success) && (
            <div className={`flex items-center gap-2 p-3 rounded-lg mb-4 text-sm ${error ? 'bg-destructive/10 text-destructive' : 'bg-green-500/10 text-green-600'}`}>
              {error ? <AlertCircle className="h-4 w-4" /> : <Check className="h-4 w-4" />}
              <span>{error || success}</span>
            </div>
          )}
          
          {activeTab === 'profile' ? (
            <form onSubmit={handleUpdateProfile} className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">昵称</label>
                <input
                  type="text"
                  value={displayName}
                  onChange={(e) => setDisplayName(e.target.value)}
                  className="w-full px-3 py-2 rounded-lg border border-input bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-primary"
                  placeholder="请输入昵称"
                />
              </div>
              <button
                type="submit"
                disabled={isLoading}
                className="w-full flex items-center justify-center gap-2 px-4 py-2.5 rounded-lg bg-primary text-primary-foreground font-medium hover:bg-primary/90 disabled:opacity-50"
              >
                {isLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}
                保存
              </button>
            </form>
          ) : (
            <form onSubmit={handleChangePassword} className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">当前密码</label>
                <input
                  type="password"
                  value={currentPassword}
                  onChange={(e) => setCurrentPassword(e.target.value)}
                  className="w-full px-3 py-2 rounded-lg border border-input bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-primary"
                  placeholder="请输入当前密码"
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">新密码</label>
                <input
                  type="password"
                  value={newPassword}
                  onChange={(e) => setNewPassword(e.target.value)}
                  className="w-full px-3 py-2 rounded-lg border border-input bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-primary"
                  placeholder="请输入新密码（至少6位）"
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">确认新密码</label>
                <input
                  type="password"
                  value={confirmPassword}
                  onChange={(e) => setConfirmPassword(e.target.value)}
                  className="w-full px-3 py-2 rounded-lg border border-input bg-background text-foreground focus:outline-none focus:ring-2 focus:ring-primary"
                  placeholder="请再次输入新密码"
                />
              </div>
              <button
                type="submit"
                disabled={isLoading}
                className="w-full flex items-center justify-center gap-2 px-4 py-2.5 rounded-lg bg-primary text-primary-foreground font-medium hover:bg-primary/90 disabled:opacity-50"
              >
                {isLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : <Key className="h-4 w-4" />}
                修改密码
              </button>
            </form>
          )}
        </div>
      </div>
    </div>
  )
}
