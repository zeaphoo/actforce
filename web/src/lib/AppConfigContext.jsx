import { createContext, useContext, useState, useEffect } from 'react'

const AppConfigContext = createContext(null)

const CONFIG_API_URL = '/api/config/app'

// 默认配置（用于加载失败时的回退）
const DEFAULT_CONFIG = {
  product_name: '轻元智能体',
  version: '0.0.0',
  release_date: '',
  allow_anonymous: true,
  ui: {
    theme: 'light',
    logo_url: '',
    favicon_url: '',
    title: '轻元智能体',
    footer_text: '',
    show_version: true,
  },
}

export function AppConfigProvider({ children }) {
  const [config, setConfig] = useState(null)
  const [isLoading, setIsLoading] = useState(true)
  const [error, setError] = useState(null)

  useEffect(() => {
    const fetchConfig = async () => {
      try {
        setIsLoading(true)
        setError(null)
        
        const response = await fetch(CONFIG_API_URL)
        if (!response.ok) {
          throw new Error(`Failed to fetch app config: ${response.status}`)
        }
        
        const data = await response.json()
        setConfig(data)
        
        // 更新页面标题
        if (data.ui?.title) {
          document.title = data.ui.title
        }
        
        // 更新 favicon
        if (data.ui?.favicon_url) {
          const link = document.querySelector("link[rel~='icon']") || document.createElement('link')
          link.rel = 'icon'
          link.href = data.ui.favicon_url
          document.head.appendChild(link)
        }
        
      } catch (e) {
        console.error('Failed to load app config:', e)
        setError(e.message)
        // 使用默认配置
        setConfig(DEFAULT_CONFIG)
      } finally {
        setIsLoading(false)
      }
    }
    
    fetchConfig()
  }, [])

  const value = {
    config: config || DEFAULT_CONFIG,
    isLoading,
    error,
    // 便捷访问器
    productName: config?.product_name || DEFAULT_CONFIG.product_name,
    version: config?.version || DEFAULT_CONFIG.version,
    releaseDate: config?.release_date || DEFAULT_CONFIG.release_date,
    allowAnonymous: config?.allow_anonymous ?? DEFAULT_CONFIG.allow_anonymous,
    ui: config?.ui || DEFAULT_CONFIG.ui,
  }

  return (
    <AppConfigContext.Provider value={value}>
      {children}
    </AppConfigContext.Provider>
  )
}

export function useAppConfig() {
  const context = useContext(AppConfigContext)
  if (!context) {
    throw new Error('useAppConfig must be used within an AppConfigProvider')
  }
  return context
}

export default AppConfigContext
