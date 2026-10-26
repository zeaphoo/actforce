import { User } from 'lucide-react'

/**
 * 用户头像组件
 * - 匿名用户显示灰色头像
 * - 登录用户显示首字母头像（管理员为琥珀色）
 */
export function UserAvatar({ user, size = 'md', className = '' }) {
  const sizeClasses = {
    sm: 'h-6 w-6 text-xs',
    md: 'h-8 w-8 text-sm',
    lg: 'h-10 w-10 text-base',
    xl: 'h-16 w-16 text-xl',
  }
  
  // 匿名用户显示灰色头像
  if (!user) {
    return (
      <div className={`${sizeClasses[size]} rounded-full bg-muted flex items-center justify-center text-muted-foreground ${className}`}>
        <User className="h-1/2 w-1/2" />
      </div>
    )
  }
  
  // 登录用户显示首字母头像
  const initial = user.display_name?.[0] || user.login_id?.[0] || 'U'
  const isAdmin = user.role === 'admin'
  
  return (
    <div className={`${sizeClasses[size]} rounded-full ${isAdmin ? 'bg-amber-500/20 text-amber-600' : 'bg-primary/20 text-primary'} flex items-center justify-center font-medium ${className}`}>
      {initial.toUpperCase()}
    </div>
  )
}
