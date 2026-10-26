import { useState, useEffect } from 'react'
import { createPortal } from 'react-dom'
import { X, CheckCircle, AlertCircle, Info } from 'lucide-react'

// Toast 类型
export const TOAST_TYPE = {
  SUCCESS: 'success',
  ERROR: 'error',
  INFO: 'info',
}

// Toast 管理器
const toastManager = {
  listeners: [],
  
  subscribe(listener) {
    this.listeners.push(listener)
    return () => {
      this.listeners = this.listeners.filter(l => l !== listener)
    }
  },
  
  notify(toast) {
    this.listeners.forEach(listener => listener(toast))
  },

  dismiss(id) {
    this.listeners.forEach(listener => listener({ id, dismiss: true }))
  },
}

// 显示 toast 的函数
export const toast = {
  success(message, duration = 3000) {
    const id = Date.now()
    toastManager.notify({ id, type: TOAST_TYPE.SUCCESS, message, duration })
    return id
  },
  
  error(message, duration = 5000) {
    const id = Date.now()
    toastManager.notify({ id, type: TOAST_TYPE.ERROR, message, duration })
    return id
  },
  
  info(message, duration = 3000) {
    const id = Date.now()
    toastManager.notify({ id, type: TOAST_TYPE.INFO, message, duration })
    return id
  },

  dismiss(id) {
    toastManager.dismiss(id)
  },
}

// Toast 容器组件
export function ToastContainer() {
  const [toasts, setToasts] = useState([])

  useEffect(() => {
    const unsubscribe = toastManager.subscribe((event) => {
      if (event.dismiss) {
        setToasts(prev => prev.filter(t => t.id !== event.id))
        return
      }
      setToasts(prev => [...prev, event])
      
      // 自动移除
      if (event.duration > 0) {
        const id = event.id
        setTimeout(() => {
          setToasts(prev => prev.filter(t => t.id !== id))
        }, event.duration)
      }
    })
    
    return unsubscribe
  }, [])

  const removeToast = (id) => {
    setToasts(prev => prev.filter(t => t.id !== id))
  }

  if (toasts.length === 0) return null

  const getIcon = (type) => {
    switch (type) {
      case TOAST_TYPE.SUCCESS:
        return <CheckCircle className="h-5 w-5 text-green-500" />
      case TOAST_TYPE.ERROR:
        return <AlertCircle className="h-5 w-5 text-red-500" />
      case TOAST_TYPE.INFO:
        return <Info className="h-5 w-5 text-blue-500" />
      default:
        return null
    }
  }

  const getStyles = (type) => {
    switch (type) {
      case TOAST_TYPE.SUCCESS:
        return 'bg-green-50 border-green-200 text-green-800 dark:bg-green-900/20 dark:border-green-800 dark:text-green-300'
      case TOAST_TYPE.ERROR:
        return 'bg-red-50 border-red-200 text-red-800 dark:bg-red-900/20 dark:border-red-800 dark:text-red-300'
      case TOAST_TYPE.INFO:
        return 'bg-blue-50 border-blue-200 text-blue-800 dark:bg-blue-900/20 dark:border-blue-800 dark:text-blue-300'
      default:
        return 'bg-gray-50 border-gray-200 text-gray-800'
    }
  }

  return createPortal(
    <div className="fixed bottom-36 left-1/2 -translate-x-1/2 z-[9999] flex flex-col gap-2 max-w-sm">
      {toasts.map((t) => (
        <div
          key={t.id}
          className={`flex items-start gap-3 p-4 rounded-lg border shadow-lg animate-in slide-in-from-right ${getStyles(t.type)}`}
        >
          {getIcon(t.type)}
          <div className="flex-1 text-sm font-medium">{t.message}</div>
          <button
            onClick={() => removeToast(t.id)}
            className="flex-shrink-0 hover:opacity-70 transition-opacity"
          >
            <X className="h-4 w-4" />
          </button>
        </div>
      ))}
    </div>,
    document.body
  )
}

export default toast
