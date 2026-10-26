import { useState } from 'react'
import { LogIn, LogOut, Settings, Shield } from 'lucide-react'
import { useAuth } from '../lib/AuthContext'
import { UserAvatar } from './UserAvatar'
import { ProfileSettingsDialog } from './ProfileSettingsDialog'
import { AdminDialog } from './AdminDialog'

/**
 * 用户菜单组件
 * - 匿名用户：显示灰色头像 + 登录按钮
 * - 登录用户：显示头像 + 下拉菜单（个人设置、退出登录）
 * - 管理员：额外显示系统管理入口
 */
export function UserMenu({ onLoginClick, adminTabs = [] }) {
  const { user, isLoggedIn, logout, isLoading } = useAuth()
  const [showMenu, setShowMenu] = useState(false)
  const [showProfileDialog, setShowProfileDialog] = useState(false)
  const [showAdminDialog, setShowAdminDialog] = useState(false)

  const isAdmin = user?.role === 'admin'

  if (isLoading) {
    return (
      <div className="h-8 w-8 rounded-full bg-muted animate-pulse" />
    )
  }

  // 匿名用户 - 显示灰色头像和登录按钮
  if (!isLoggedIn) {
    return (
      <div className="flex items-center gap-2">
        <UserAvatar user={null} size="md" />
        <button
          onClick={onLoginClick}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
        >
          <LogIn className="h-4 w-4" />
          <span>登录</span>
        </button>
      </div>
    )
  }

  // 登录用户 - 显示头像和下拉菜单
  return (
    <>
      <div className="relative">
        <button
          onClick={() => setShowMenu(!showMenu)}
          className="flex items-center gap-2 px-2 py-1 rounded-lg hover:bg-muted transition-colors"
        >
          <UserAvatar user={user} size="md" />
          <span className="text-sm text-foreground max-w-[100px] truncate">
            {user?.display_name || user?.login_id || '用户'}
          </span>
        </button>
        
        {showMenu && (
          <>
            <div 
              className="fixed inset-0 z-40" 
              onClick={() => setShowMenu(false)} 
            />
            <div className="absolute right-0 top-full mt-1 z-50 w-56 rounded-lg border border-border bg-card shadow-lg py-1">
              {/* User Info */}
              <div className="px-3 py-2 border-b border-border">
                <p className="text-sm font-medium text-foreground truncate">
                  {user?.display_name || user?.login_id}
                </p>
                <p className="text-xs text-muted-foreground">{user?.login_id}</p>
              </div>
              
              {/* Menu Items */}
              <div className="py-1">
                <button
                  onClick={() => {
                    setShowMenu(false)
                    setShowProfileDialog(true)
                  }}
                  className="w-full px-3 py-2 text-left text-sm text-foreground hover:bg-muted transition-colors flex items-center gap-2"
                >
                  <Settings className="h-4 w-4" />
                  个人设置
                </button>
                
                {isAdmin && (
                  <button
                    onClick={() => {
                      setShowMenu(false)
                      setShowAdminDialog(true)
                    }}
                    className="w-full px-3 py-2 text-left text-sm text-foreground hover:bg-muted transition-colors flex items-center gap-2"
                  >
                    <Shield className="h-4 w-4 text-amber-500" />
                    系统管理
                  </button>
                )}
              </div>
              
              {/* Logout */}
              <div className="border-t border-border pt-1">
                <button
                  onClick={() => {
                    setShowMenu(false)
                    logout()
                  }}
                  className="w-full px-3 py-2 text-left text-sm text-destructive hover:bg-muted transition-colors flex items-center gap-2"
                >
                  <LogOut className="h-4 w-4" />
                  退出登录
                </button>
              </div>
            </div>
          </>
        )}
      </div>
      
      {/* Dialogs */}
      <ProfileSettingsDialog 
        isOpen={showProfileDialog} 
        onClose={() => setShowProfileDialog(false)} 
      />
      
      {isAdmin && (
        <AdminDialog 
          isOpen={showAdminDialog} 
          onClose={() => setShowAdminDialog(false)} 
          extraTabs={adminTabs}
        />
      )}
    </>
  )
}
