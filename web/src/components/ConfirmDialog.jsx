import { AlertTriangle } from 'lucide-react'

/**
 * 确认对话框组件
 * 用于需要用户确认的操作(如删除、重置等)
 * 
 * @param {boolean} isOpen - 是否显示对话框
 * @param {Function} onClose - 关闭对话框回调
 * @param {Function} onConfirm - 确认操作回调
 * @param {string} title - 对话框标题
 * @param {string} message - 确认消息内容
 * @param {string} confirmText - 确认按钮文本
 * @param {string} cancelText - 取消按钮文本
 * @param {string} variant - 变体类型: 'destructive'(破坏性操作) | 'warning'(警告) | 'info'(信息)
 */
export function ConfirmDialog({
  isOpen,
  onClose,
  onConfirm,
  title = '确认操作',
  message,
  confirmText = '确认',
  cancelText = '取消',
  variant = 'destructive',
}) {
  if (!isOpen) return null

  const variantStyles = {
    destructive: {
      icon: 'bg-destructive/10 text-destructive',
      button: 'bg-destructive text-destructive-foreground hover:bg-destructive/90',
    },
    warning: {
      icon: 'bg-amber-500/10 text-amber-500',
      button: 'bg-amber-500 text-white hover:bg-amber-600',
    },
    info: {
      icon: 'bg-primary/10 text-primary',
      button: 'bg-primary text-primary-foreground hover:bg-primary/90',
    },
  }

  const style = variantStyles[variant] || variantStyles.destructive

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* 背景遮罩 */}
      <div 
        className="absolute inset-0 bg-black/60"
        onClick={onClose}
      />
      
      {/* 对话框内容 */}
      <div className="relative bg-card border border-border rounded-lg shadow-xl w-full max-w-md mx-4 p-6">
        <div className="flex items-start gap-3">
          {/* 图标 */}
          <div className={`flex-shrink-0 w-10 h-10 rounded-full flex items-center justify-center ${style.icon}`}>
            <AlertTriangle className="w-5 h-5" />
          </div>
          
          {/* 消息内容 */}
          <div className="flex-1">
            <h3 className="text-lg font-semibold text-foreground mb-2">{title}</h3>
            <p className="text-sm text-muted-foreground">{message}</p>
          </div>
        </div>
        
        {/* 按钮组 */}
        <div className="flex justify-end gap-2 mt-6">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm font-medium text-muted-foreground hover:text-foreground transition-colors"
          >
            {cancelText}
          </button>
          <button
            onClick={() => {
              onConfirm()
              onClose()
            }}
            className={`px-4 py-2 text-sm font-medium rounded-lg transition-colors ${style.button}`}
          >
            {confirmText}
          </button>
        </div>
      </div>
    </div>
  )
}
