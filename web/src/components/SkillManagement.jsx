import { useState, useEffect } from 'react'
import { ToggleLeft, ToggleRight, Loader2 } from 'lucide-react'
import { getAuthHeaders } from '../lib/auth'

/**
 * 技能管理组件
 * 只显示内置技能，支持启用/停用
 */
export function SkillManagement({ onError, onSuccess }) {
  const [builtinSkills, setBuiltinSkills] = useState([])
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState('')

  const handleError = (msg) => {
    setError(msg)
    onError?.(msg)
    setTimeout(() => setError(''), 5000)
  }

  const handleSuccess = (msg) => {
    onSuccess?.(msg)
  }

  const fetchBuiltinSkills = async () => {
    try {
      const response = await fetch('/api/skills/builtin', {
        headers: getAuthHeaders(),
      })
      if (response.ok) {
        const data = await response.json()
        setBuiltinSkills(Array.isArray(data) ? data : (data.skills || []))
      }
    } catch (e) {
      // 静默失败
    }
  }

  useEffect(() => {
    setIsLoading(true)
    fetchBuiltinSkills().finally(() => setIsLoading(false))
  }, [])

  const toggleBuiltinSkill = async (skillId, currentStatus) => {
    try {
      const response = await fetch(`/api/skills/builtin/${skillId}`, {
        method: 'PUT',
        headers: {
          ...getAuthHeaders(),
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          is_enabled: !currentStatus,
        }),
      })

      if (response.ok) {
        handleSuccess(currentStatus ? '技能已停用' : '技能已启用')
        fetchBuiltinSkills()
      } else {
        handleError('更新技能状态失败')
      }
    } catch (e) {
      handleError('更新技能状态失败')
    }
  }

  return (
    <div className="space-y-4">
      {error && (
        <div className="p-3 rounded-lg bg-destructive/10 text-destructive text-sm">
          {error}
        </div>
      )}

      <BuiltinSkillsTab
        skills={builtinSkills}
        isLoading={isLoading}
        onToggle={toggleBuiltinSkill}
      />
    </div>
  )
}

function BuiltinSkillsTab({ skills, isLoading, onToggle }) {
  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
        <span className="ml-2 text-muted-foreground">加载中...</span>
      </div>
    )
  }

  if (skills.length === 0) {
    return (
      <div className="text-center py-12 text-muted-foreground">
        暂无内置技能
      </div>
    )
  }

  return (
    <div className="space-y-2">
      {skills.map((skill) => (
        <div
          key={skill.id}
          className="p-4 rounded-lg border border-border hover:bg-muted/50 transition-colors"
        >
          <div className="flex items-start justify-between gap-4">
            <div className="flex-1">
              <h4 className="font-medium text-foreground">{skill.name}</h4>
              {skill.description && (
                <p className="text-sm text-muted-foreground mt-1">
                  {skill.description}
                </p>
              )}
            </div>
            <button
              onClick={() => onToggle(skill.id, skill.is_enabled)}
              className="flex-shrink-0"
              title={skill.is_enabled ? '停用' : '启用'}
            >
              {skill.is_enabled ? (
                <ToggleRight className="h-6 w-6 text-green-500" />
              ) : (
                <ToggleLeft className="h-6 w-6 text-muted-foreground" />
              )}
            </button>
          </div>
        </div>
      ))}
    </div>
  )
}
