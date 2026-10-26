import { useState } from 'react'
import { Plus, Key, UserX, UserCheck, Trash2, X } from 'lucide-react'
import { UserAvatar } from './UserAvatar'
import { getAuthHeaders, md5 } from '../lib/auth'
import { ConfirmDialog } from './ConfirmDialog'

/** 重置密码对话框 */
function ResetPasswordDialog({ isOpen, userName, onClose, onConfirm }) {
  const [password, setPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [error, setError] = useState('')

  const handleSubmit = () => {
    setError('')
    if (!password.trim()) {
      setError('请输入新密码')
      return
    }
    if (password !== confirmPassword) {
      setError('两次输入的密码不一致')
      return
    }
    onConfirm(password)
  }

  const handleClose = () => {
    setPassword('')
    setConfirmPassword('')
    setError('')
    onClose()
  }

  if (!isOpen) return null

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={handleClose}>
      <div className="bg-background rounded-xl border border-border shadow-xl w-full max-w-sm p-6" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold">重置密码</h3>
          <button onClick={handleClose} className="p-1 hover:bg-muted rounded"><X className="h-4 w-4" /></button>
        </div>
        <p className="text-sm text-muted-foreground mb-4">为用户 <span className="font-medium text-foreground">{userName}</span> 设置新密码</p>
        <div className="space-y-3">
          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">新密码</label>
            <input
              type="password"
              value={password}
              onChange={e => setPassword(e.target.value)}
              className="w-full px-3 py-2 text-sm rounded-lg border border-input bg-background"
              placeholder="输入新密码"
              autoFocus
            />
          </div>
          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">确认密码</label>
            <input
              type="password"
              value={confirmPassword}
              onChange={e => setConfirmPassword(e.target.value)}
              className="w-full px-3 py-2 text-sm rounded-lg border border-input bg-background"
              placeholder="再次输入新密码"
            />
          </div>
          {error && <p className="text-sm text-destructive">{error}</p>}
        </div>
        <div className="flex justify-end gap-2 mt-4">
          <button onClick={handleClose} className="px-4 py-2 text-sm text-muted-foreground hover:text-foreground rounded-lg">取消</button>
          <button onClick={handleSubmit} className="px-4 py-2 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90">确认重置</button>
        </div>
      </div>
    </div>
  )
}

/** 添加用户对话框 */
function AddUserDialog({ isOpen, onClose, onConfirm }) {
  const [loginId, setLoginId] = useState('')
  const [password, setPassword] = useState('')
  const [displayName, setDisplayName] = useState('')
  const [role, setRole] = useState('user')
  const [error, setError] = useState('')

  const handleSubmit = () => {
    setError('')
    if (!loginId.trim() || !password.trim()) {
      setError('请填写用户名和密码')
      return
    }
    onConfirm({ login_id: loginId.trim(), password, display_name: displayName.trim(), role })
  }

  const handleClose = () => {
    setLoginId('')
    setPassword('')
    setDisplayName('')
    setRole('user')
    setError('')
    onClose()
  }

  if (!isOpen) return null

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={handleClose}>
      <div className="bg-background rounded-xl border border-border shadow-xl w-full max-w-sm p-6" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold">添加用户</h3>
          <button onClick={handleClose} className="p-1 hover:bg-muted rounded"><X className="h-4 w-4" /></button>
        </div>
        <div className="space-y-3">
          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">用户名 *</label>
            <input
              type="text"
              value={loginId}
              onChange={e => setLoginId(e.target.value)}
              className="w-full px-3 py-2 text-sm rounded-lg border border-input bg-background"
              placeholder="登录用户名"
              autoFocus
            />
          </div>
          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">密码 *</label>
            <input
              type="password"
              value={password}
              onChange={e => setPassword(e.target.value)}
              className="w-full px-3 py-2 text-sm rounded-lg border border-input bg-background"
              placeholder="初始密码"
            />
          </div>
          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">显示名称</label>
            <input
              type="text"
              value={displayName}
              onChange={e => setDisplayName(e.target.value)}
              className="w-full px-3 py-2 text-sm rounded-lg border border-input bg-background"
              placeholder="可选"
            />
          </div>
          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">角色</label>
            <select
              value={role}
              onChange={e => setRole(e.target.value)}
              className="w-full px-3 py-2 text-sm rounded-lg border border-input bg-background"
            >
              <option value="user">普通用户</option>
              <option value="admin">管理员</option>
            </select>
          </div>
          {error && <p className="text-sm text-destructive">{error}</p>}
        </div>
        <div className="flex justify-end gap-2 mt-4">
          <button onClick={handleClose} className="px-4 py-2 text-sm text-muted-foreground hover:text-foreground rounded-lg">取消</button>
          <button onClick={handleSubmit} className="px-4 py-2 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90">创建</button>
        </div>
      </div>
    </div>
  )
}

/**
 * 用户管理组件
 */
export function UserManagement({ onError, onSuccess }) {
  const [users, setUsers] = useState([])
  const [isLoading, setIsLoading] = useState(false)
  const [isAddUserOpen, setIsAddUserOpen] = useState(false)
  const [resetPasswordUser, setResetPasswordUser] = useState(null) // { userId, userName }
  const [deleteUserConfirm, setDeleteUserConfirm] = useState(null) // { userId, loginId }

  const fetchUsers = async () => {
    setIsLoading(true)
    try {
      const response = await fetch('/api/auth/users', {
        headers: getAuthHeaders(),
      })
      if (response.ok) {
        const data = await response.json()
        setUsers(data.users || [])
      }
    } catch (e) {
      onError('获取用户列表失败')
    } finally {
      setIsLoading(false)
    }
  }

  const handleCreateUser = async (newUser) => {
    try {
      // MD5 hash the password before sending to backend
      const userData = {
        ...newUser,
        password: md5(newUser.password),
      }
      
      const response = await fetch('/api/auth/users', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...getAuthHeaders() },
        body: JSON.stringify(userData),
      })
      
      if (!response.ok) {
        const data = await response.json()
        throw new Error(data.detail || '创建用户失败')
      }
      
      onSuccess('用户创建成功')
      setIsAddUserOpen(false)
      fetchUsers()
    } catch (e) {
      onError(e.message)
    }
  }

  const handleUpdateUser = async (userId, updates) => {
    try {
      const response = await fetch(`/api/auth/users/${userId}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json', ...getAuthHeaders() },
        body: JSON.stringify(updates),
      })
      
      if (!response.ok) {
        const data = await response.json()
        throw new Error(data.detail || '更新用户失败')
      }
      
      onSuccess('用户更新成功')
      fetchUsers()
    } catch (e) {
      onError(e.message)
    }
  }

  const handleResetPassword = async (userId, password) => {
    try {
      const passwordMd5 = md5(password)
      const response = await fetch(`/api/auth/users/${userId}/reset-password`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...getAuthHeaders() },
        body: JSON.stringify({ new_password: passwordMd5 }),
      })
      if (!response.ok) {
        const data = await response.json()
        throw new Error(data.detail || '重置密码失败')
      }
      onSuccess('密码重置成功')
      setResetPasswordUser(null)
    } catch (e) {
      onError(e.message)
    }
  }

  const handleDeleteUser = async (userId, loginId) => {
    setDeleteUserConfirm({ userId, loginId })
  }

  const confirmDeleteUser = async () => {
    if (!deleteUserConfirm) return
    
    const { userId, loginId } = deleteUserConfirm
    setDeleteUserConfirm(null)
    
    try {
      const response = await fetch(`/api/auth/users/${userId}`, {
        method: 'DELETE',
        headers: getAuthHeaders(),
      })
      
      if (!response.ok) {
        const data = await response.json()
        throw new Error(data.detail || '删除用户失败')
      }
      
      onSuccess('用户已删除')
      fetchUsers()
    } catch (e) {
      onError(e.message)
    }
  }

  const handleToggleUserActive = async (userId, currentActive) => {
    await handleUpdateUser(userId, { is_active: !currentActive })
  }

  // 初始加载
  useState(() => {
    fetchUsers()
  }, [])

  return (
    <div className="space-y-4">
      {/* 添加用户按钮 */}
      <div className="flex justify-between items-center">
        <h3 className="text-sm font-medium text-foreground">用户列表 ({users.length})</h3>
        <button
          onClick={() => setIsAddUserOpen(true)}
          className="flex items-center gap-1 px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90"
        >
          <Plus className="h-4 w-4" />
          添加用户
        </button>
      </div>
      
      {/* 用户列表 */}
      {isLoading ? (
        <p className="text-center text-muted-foreground py-8">加载中...</p>
      ) : users.length === 0 ? (
        <p className="text-center text-muted-foreground py-8">暂无用户</p>
      ) : (
        <div className="space-y-2">
          {users.map((u) => (
            <div key={u.id} className={`p-3 rounded-lg border border-border hover:bg-muted/50 ${u.is_active === false ? 'opacity-60' : ''}`}>
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <UserAvatar user={u} size="md" />
                  <div>
                    <div className="flex items-center gap-2">
                      <p className="font-medium text-foreground">{u.display_name || u.login_id}</p>
                      {u.is_active === false && (
                        <span className="px-1.5 py-0.5 rounded text-xs bg-red-500/20 text-red-600">已冻结</span>
                      )}
                    </div>
                    <p className="text-xs text-muted-foreground">{u.login_id}</p>
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <span className={`px-2 py-0.5 rounded text-xs ${u.role === 'admin' ? 'bg-amber-500/20 text-amber-600' : 'bg-muted text-muted-foreground'}`}>
                    {u.role === 'admin' ? '管理员' : '用户'}
                  </span>
                  {/* 操作按钮 */}
                  <div className="flex items-center gap-1 ml-2">
                    <button
                      onClick={() => setResetPasswordUser({ userId: u.id, userName: u.display_name || u.login_id })}
                      className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted rounded"
                      title="重置密码"
                    >
                      <Key className="h-4 w-4" />
                    </button>
                    <button
                      onClick={() => handleToggleUserActive(u.id, u.is_active !== false)}
                      className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted rounded"
                      title={u.is_active === false ? '解冻用户' : '冻结用户'}
                    >
                      {u.is_active === false ? <UserCheck className="h-4 w-4" /> : <UserX className="h-4 w-4" />}
                    </button>
                    <button
                      onClick={() => handleDeleteUser(u.id, u.login_id)}
                      className="p-1.5 text-muted-foreground hover:text-destructive hover:bg-muted rounded"
                      title="删除用户"
                    >
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </div>
                </div>
              </div>
              
            </div>
          ))}
        </div>
      )}

      {/* 重置密码对话框 */}
      <ResetPasswordDialog
        isOpen={!!resetPasswordUser}
        userName={resetPasswordUser?.userName || ''}
        onClose={() => setResetPasswordUser(null)}
        onConfirm={(password) => {
          if (resetPasswordUser) {
            handleResetPassword(resetPasswordUser.userId, password)
          }
        }}
      />

      {/* 添加用户对话框 */}
      <AddUserDialog
        isOpen={isAddUserOpen}
        onClose={() => setIsAddUserOpen(false)}
        onConfirm={handleCreateUser}
      />

      {/* 删除用户确认对话框 */}
      <ConfirmDialog
        isOpen={!!deleteUserConfirm}
        onClose={() => setDeleteUserConfirm(null)}
        onConfirm={confirmDeleteUser}
        title="确认删除用户"
        message={deleteUserConfirm ? `确定要删除用户 "${deleteUserConfirm.loginId}" 吗？此操作不可恢复。` : ''}
        confirmText="删除"
        cancelText="取消"
        variant="destructive"
      />
    </div>
  )
}
