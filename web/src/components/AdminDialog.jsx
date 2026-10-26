import { useState } from 'react'
import { X, Shield, Users, Wrench, Activity, Settings, Loader2, MessageSquare } from 'lucide-react'
import { toast } from './Toast'
import { UserManagement } from './UserManagement'
import { SkillManagement } from './SkillManagement'
import { SystemStatus } from './SystemStatus'
import { SystemSettings } from './SystemSettings'
import { QuickPromptManagement } from './QuickPromptManagement'

/**
 * 管理员管理对话框
 * - 用户管理
 * - 技能管理
 * - 系统状态
 * - 系统设置
 * - extraTabs: 私有产品注入的扩展页（如许可证管理）
 */
export function AdminDialog({ isOpen, onClose, extraTabs = [] }) {
  const [activeTab, setActiveTab] = useState('users')

  const [isLoading, setIsLoading] = useState(false)

  const handleError = (msg) => {
    toast.error(msg)
  }

  const handleSuccess = (msg) => {
    toast.success(msg)
  }

  const handleTabChange = (tab) => {
    setActiveTab(tab)
  }

  if (!isOpen) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/60" onClick={onClose} />
      
      <div className="relative bg-card border border-border rounded-lg shadow-xl w-full max-w-5xl mx-4 h-[80vh] max-h-[800px] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-border">
          <div className="flex items-center gap-2">
            <Shield className="h-5 w-5 text-amber-500" />
            <h2 className="text-lg font-semibold text-foreground">系统管理</h2>
          </div>
          <button onClick={onClose} className="text-muted-foreground hover:text-foreground">
            <X className="h-5 w-5" />
          </button>
        </div>
        
        {/* Body with left sidebar tabs */}
        <div className="flex-1 flex overflow-hidden">
          {/* Left Sidebar Tabs */}
          <div className="w-48 border-r border-border bg-muted/30 py-2">
            <button
              onClick={() => handleTabChange('users')}
              className={`w-full px-4 py-3 text-sm font-medium transition-colors flex items-center gap-3 ${activeTab === 'users' ? 'bg-primary/10 text-primary border-r-2 border-primary' : 'text-muted-foreground hover:text-foreground hover:bg-muted'}`}
            >
              <Users className="h-4 w-4" />
              用户管理
            </button>
            <button
              onClick={() => handleTabChange('quick-prompts')}
              className={`w-full px-4 py-3 text-sm font-medium transition-colors flex items-center gap-3 ${activeTab === 'quick-prompts' ? 'bg-primary/10 text-primary border-r-2 border-primary' : 'text-muted-foreground hover:text-foreground hover:bg-muted'}`}
            >
              <MessageSquare className="h-4 w-4" />
              快捷提示
            </button>
            <button
              onClick={() => handleTabChange('skills')}
              className={`w-full px-4 py-3 text-sm font-medium transition-colors flex items-center gap-3 ${activeTab === 'skills' ? 'bg-primary/10 text-primary border-r-2 border-primary' : 'text-muted-foreground hover:text-foreground hover:bg-muted'}`}
            >
              <Wrench className="h-4 w-4" />
              技能管理
            </button>
            <button
              onClick={() => handleTabChange('settings')}
              className={`w-full px-4 py-3 text-sm font-medium transition-colors flex items-center gap-3 ${activeTab === 'settings' ? 'bg-primary/10 text-primary border-r-2 border-primary' : 'text-muted-foreground hover:text-foreground hover:bg-muted'}`}
            >
              <Settings className="h-4 w-4" />
              系统设置
            </button>
            <button
              onClick={() => handleTabChange('system')}
              className={`w-full px-4 py-3 text-sm font-medium transition-colors flex items-center gap-3 ${activeTab === 'system' ? 'bg-primary/10 text-primary border-r-2 border-primary' : 'text-muted-foreground hover:text-foreground hover:bg-muted'}`}
            >
              <Activity className="h-4 w-4" />
              系统状态
            </button>

            {extraTabs.map((tab) => {
              const Icon = tab.icon
              return (
                <button
                  key={tab.id}
                  onClick={() => handleTabChange(tab.id)}
                  className={`w-full px-4 py-3 text-sm font-medium transition-colors flex items-center gap-3 ${activeTab === tab.id ? 'bg-primary/10 text-primary border-r-2 border-primary' : 'text-muted-foreground hover:text-foreground hover:bg-muted'}`}
                >
                  {Icon ? <Icon className="h-4 w-4" /> : null}
                  {tab.label}
                </button>
              )
            })}
          </div>
          
          {/* Content */}
          <div className="flex-1 overflow-auto p-6">

            
            {isLoading ? (
              <div className="flex items-center justify-center py-12">
                <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
              </div>
            ) : (
              <>
                {activeTab === 'users' && (
                  <UserManagement onError={handleError} onSuccess={handleSuccess} />
                )}
                {activeTab === 'quick-prompts' && (
                  <QuickPromptManagement onError={handleError} onSuccess={handleSuccess} />
                )}
                {activeTab === 'skills' && (
                  <SkillManagement onError={handleError} />
                )}
                {activeTab === 'system' && (
                  <SystemStatus onError={handleError} />
                )}
                {activeTab === 'settings' && (
                  <SystemSettings onError={handleError} onSuccess={handleSuccess} />
                )}
                {extraTabs.map((tab) => {
                  if (activeTab !== tab.id) return null
                  const Component = tab.component
                  return (
                    <Component
                      key={tab.id}
                      onError={handleError}
                      onSuccess={handleSuccess}
                      {...(tab.props || {})}
                    />
                  )
                })}
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
