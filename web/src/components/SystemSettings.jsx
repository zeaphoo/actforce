import { useState, useEffect } from 'react'
import { Loader2, Check, ToggleLeft, ToggleRight } from 'lucide-react'
import { getAuthHeaders } from '../lib/auth'
import { NumberInput } from './ui/number-input'

/**
 * 系统设置组件
 */
export function SystemSettings({ onError, onSuccess }) {
  const [systemSettings, setSystemSettings] = useState({})
  const [settingsForm, setSettingsForm] = useState({
    allow_anonymous: true,
    conversation_history_retention_days: 30,
    max_concurrent_sessions: 5,
    global_llm_requests: 24,
  })
  const [isSavingSettings, setIsSavingSettings] = useState(false)
  const [isLoading, setIsLoading] = useState(false)

  const fetchSystemSettings = async () => {
    setIsLoading(true)
    try {
      const response = await fetch('/api/config/settings', {
        headers: getAuthHeaders(),
      })

      if (response.ok) {
        const data = await response.json()
        setSystemSettings(data.settings || {})

        // 更新表单数据（确保布尔值类型正确）
        const getBool = (val, def) => typeof val === 'boolean' ? val : (val === 'true' ? true : val === 'false' ? false : def)
        setSettingsForm({
          allow_anonymous: getBool(data.settings?.allow_anonymous?.value, true),
          conversation_history_retention_days: data.settings?.conversation_history_retention_days?.value ?? 30,
          max_concurrent_sessions: data.settings?.max_concurrent_sessions?.value ?? 5,
          global_llm_requests: data.settings?.global_llm_requests?.value ?? 24,
        })
      }
    } catch (e) {
      onError('获取系统设置失败')
    } finally {
      setIsLoading(false)
    }
  }

  const handleSaveSettings = async () => {
    setIsSavingSettings(true)
    try {
      const response = await fetch('/api/config/settings/batch', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...getAuthHeaders() },
        body: JSON.stringify(settingsForm),
      })

      if (!response.ok) {
        const data = await response.json()
        throw new Error(data.detail || '保存系统设置失败')
      }

      onSuccess('系统设置保存成功')
      await fetchSystemSettings()
    } catch (e) {
      onError(e.message)
    } finally {
      setIsSavingSettings(false)
    }
  }

  const handleSettingToggle = (key) => {
    setSettingsForm({
      ...settingsForm,
      [key]: !settingsForm[key],
    })
  }

  const handleSettingChange = (key, value) => {
    setSettingsForm({
      ...settingsForm,
      [key]: value,
    })
  }

  useEffect(() => {
    fetchSystemSettings()
  }, [])

  return (
    <div className="space-y-4">
      {/* 访问控制 */}
      <div className="p-4 rounded-lg border border-border">
        <h3 className="font-medium text-foreground mb-4">访问控制</h3>
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <div>
              <p className="font-medium text-foreground">允许匿名访问</p>
              <p className="text-sm text-muted-foreground mt-1">未登录用户也可以使用系统功能</p>
            </div>
            <button
              onClick={() => handleSettingToggle('allow_anonymous')}
              className="text-2xl"
            >
              {settingsForm.allow_anonymous ? (
                <ToggleRight className="h-8 w-8 text-green-500" />
              ) : (
                <ToggleLeft className="h-8 w-8 text-gray-400" />
              )}
            </button>
          </div>
        </div>
      </div>

      {/* 数据保留 */}
      <div className="p-4 rounded-lg border border-border">
        <h3 className="font-medium text-foreground mb-4">数据保留</h3>
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-foreground mb-2">
              对话历史记录保留时间（天）
            </label>
            {isLoading ? (
              <p className="text-sm text-muted-foreground">加载中...</p>
            ) : (
              <>
                <NumberInput
                  value={settingsForm.conversation_history_retention_days}
                  onChange={(v) => {
                    const num = typeof v === 'number' ? v : 0
                    handleSettingChange('conversation_history_retention_days', num)
                  }}
                  min={0}
                  placeholder="0 表示永久保留"
                />
                <p className="text-xs text-muted-foreground mt-1">设置为 0 表示永久保留对话历史</p>
              </>
            )}
          </div>
        </div>
      </div>

      {/* 会话控制 */}
      <div className="p-4 rounded-lg border border-border">
        <h3 className="font-medium text-foreground mb-4">会话控制</h3>
        <div className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-foreground mb-2">
              最大并发会话数量
            </label>
            {isLoading ? (
              <p className="text-sm text-muted-foreground">加载中...</p>
            ) : (
              <>
                <NumberInput
                  value={settingsForm.max_concurrent_sessions}
                  onChange={(v) => {
                    const num = typeof v === 'number' ? v : 1
                    handleSettingChange('max_concurrent_sessions', num)
                  }}
                  min={1}
                  max={100}
                  placeholder="同时运行的最大会话数"
                />
                <p className="text-xs text-muted-foreground mt-1">系统允许同时运行的最大会话数量</p>
              </>
            )}
          </div>
          <div>
            <label className="block text-sm font-medium text-foreground mb-2">
              全局 LLM 最大并发请求数量
            </label>
            {isLoading ? (
              <p className="text-sm text-muted-foreground">加载中...</p>
            ) : (
              <>
                <NumberInput
                  value={settingsForm.global_llm_requests}
                  onChange={(v) => {
                    const num = typeof v === 'number' ? v : 1
                    handleSettingChange('global_llm_requests', num)
                  }}
                  min={1}
                  max={100}
                  placeholder="全局最大 LLM 并发请求数"
                />
                <p className="text-xs text-muted-foreground mt-1">所有会话共享的最大 LLM 请求并发数</p>
              </>
            )}
          </div>
        </div>
      </div>

      {/* 保存按钮 */}
      <div className="flex justify-end pt-4 border-t border-border">
        <button
          onClick={handleSaveSettings}
          className="px-4 py-2 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 flex items-center gap-2"
          disabled={isSavingSettings}
        >
          {isSavingSettings ? (
            <>
              <Loader2 className="h-4 w-4 animate-spin" />
              保存中...
            </>
          ) : (
            <>
              <Check className="h-4 w-4" />
              保存设置
            </>
          )}
        </button>
      </div>
    </div>
  )
}
