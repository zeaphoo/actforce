import { useState, useEffect } from 'react'
import { Plus, Trash2, Edit2, ToggleRight, ToggleLeft, ChevronUp, ChevronDown, Sparkles, Code, FileQuestion, Lightbulb, Book, MessageSquare, Pen, Search, Star, Zap, Brain, Rocket, Target, Award, Heart } from 'lucide-react'
import { getAuthHeaders } from '../lib/auth'
import { ConfirmDialog } from './ConfirmDialog'

/**
 * 快捷提示管理组件
 */
export function QuickPromptManagement({ onError, onSuccess }) {
  const [prompts, setPrompts] = useState([])
  const [isLoading, setIsLoading] = useState(false)
  const [showAddForm, setShowAddForm] = useState(false)
  const [editingId, setEditingId] = useState(null)
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(null) // { id, title }
  const [formData, setFormData] = useState({
    title: '',
    description: '',
    prompt: '',
    icon: 'Sparkles',
    category: '',
    sort_order: 0,
    is_enabled: true,
  })

  const iconOptions = [
    { name: 'Sparkles', icon: Sparkles },
    { name: 'Code', icon: Code },
    { name: 'FileQuestion', icon: FileQuestion },
    { name: 'Lightbulb', icon: Lightbulb },
    { name: 'Book', icon: Book },
    { name: 'MessageSquare', icon: MessageSquare },
    { name: 'Pen', icon: Pen },
    { name: 'Search', icon: Search },
    { name: 'Star', icon: Star },
    { name: 'Zap', icon: Zap },
    { name: 'Brain', icon: Brain },
    { name: 'Rocket', icon: Rocket },
    { name: 'Target', icon: Target },
    { name: 'Award', icon: Award },
    { name: 'Heart', icon: Heart },
  ]

  const renderIcon = (iconName, size = 'w-5 h-5') => {
    const IconComponent = iconOptions.find(opt => opt.name === iconName)?.icon || Sparkles
    return <IconComponent className={size} />
  }

  const fetchPrompts = async () => {
    setIsLoading(true)
    try {
      const response = await fetch('/api/quick-prompts/?include_disabled=true', {
        headers: getAuthHeaders(),
      })
      
      if (response.ok) {
        const data = await response.json()
        setPrompts(data.prompts || [])
      }
    } catch (e) {
      onError('获取快捷提示失败')
    } finally {
      setIsLoading(false)
    }
  }

  const handleCreate = async (e) => {
    e.preventDefault()
    
    if (!formData.title.trim() || !formData.prompt.trim()) {
      onError('请填写标题和提示内容')
      return
    }
    
    try {
      const response = await fetch('/api/quick-prompts/', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...getAuthHeaders() },
        body: JSON.stringify(formData),
      })
      
      if (!response.ok) {
        const data = await response.json()
        throw new Error(data.detail || '创建失败')
      }
      
      onSuccess('快捷提示创建成功')
      setShowAddForm(false)
      resetForm()
      fetchPrompts()
    } catch (e) {
      onError(e.message)
    }
  }

  const handleUpdate = async (id) => {
    try {
      const response = await fetch(`/api/quick-prompts/${id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json', ...getAuthHeaders() },
        body: JSON.stringify(formData),
      })
      
      if (!response.ok) {
        const data = await response.json()
        throw new Error(data.detail || '更新失败')
      }
      
      onSuccess('快捷提示更新成功')
      setEditingId(null)
      resetForm()
      fetchPrompts()
    } catch (e) {
      onError(e.message)
    }
  }

  const handleDelete = async (id, title) => {
    setShowDeleteConfirm({ id, title })
  }

  const confirmDelete = async () => {
    if (!showDeleteConfirm) return
    
    const { id, title } = showDeleteConfirm
    setShowDeleteConfirm(null)
    
    try {
      const response = await fetch(`/api/quick-prompts/${id}`, {
        method: 'DELETE',
        headers: getAuthHeaders(),
      })
      
      if (!response.ok) {
        const data = await response.json()
        throw new Error(data.detail || '删除失败')
      }
      
      onSuccess('快捷提示已删除')
      fetchPrompts()
    } catch (e) {
      onError(e.message)
    }
  }

  const cancelDelete = () => {
    setShowDeleteConfirm(null)
  }

  const handleToggleEnabled = async (id, currentEnabled) => {
    try {
      const response = await fetch(`/api/quick-prompts/${id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json', ...getAuthHeaders() },
        body: JSON.stringify({ is_enabled: !currentEnabled }),
      })
      
      if (!response.ok) {
        throw new Error('更新失败')
      }
      
      fetchPrompts()
    } catch (e) {
      onError(e.message)
    }
  }

  const handleChangeSort = async (id, currentOrder, direction) => {
    try {
      const response = await fetch(`/api/quick-prompts/${id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json', ...getAuthHeaders() },
        body: JSON.stringify({ sort_order: currentOrder + direction }),
      })
      
      if (!response.ok) {
        throw new Error('更新排序失败')
      }
      
      fetchPrompts()
    } catch (e) {
      onError(e.message)
    }
  }

  const handleEdit = (prompt) => {
    setEditingId(prompt.id)
    setFormData({
      title: prompt.title,
      description: prompt.description || '',
      prompt: prompt.prompt,
      icon: prompt.icon || 'Sparkles',
      category: prompt.category || '',
      sort_order: prompt.sort_order,
      is_enabled: prompt.is_enabled,
    })
    setShowAddForm(false)
  }

  const handleCancel = () => {
    setShowAddForm(false)
    setEditingId(null)
    resetForm()
  }

  const resetForm = () => {
    setFormData({
      title: '',
      description: '',
      prompt: '',
      icon: 'Sparkles',
      category: '',
      sort_order: prompts.length > 0 ? Math.max(...prompts.map(p => p.sort_order)) + 1 : 0,
      is_enabled: true,
    })
  }

  useEffect(() => {
    fetchPrompts()
  }, [])

  return (
    <div className="space-y-4">
      {/* 添加按钮 */}
      <div className="flex justify-between items-center">
        <h3 className="text-sm font-medium text-foreground">快捷提示列表 ({prompts.length})</h3>
        <button
          onClick={() => { resetForm(); setShowAddForm(!showAddForm); setEditingId(null) }}
          className="flex items-center gap-1 px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90"
        >
          <Plus className="h-4 w-4" />
          添加提示
        </button>
      </div>

      {/* 添加/编辑表单 */}
      {(showAddForm || editingId) && (
        <form onSubmit={editingId ? (e) => handleUpdate(editingId) : handleCreate} className="p-4 rounded-lg border border-border bg-muted/30 space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium text-muted-foreground mb-1">标题 *</label>
              <input
                type="text"
                value={formData.title}
                onChange={(e) => setFormData({ ...formData, title: e.target.value })}
                className="w-full px-3 py-2 text-sm rounded-lg border border-input bg-background"
                placeholder="例如：创意写作"
                required
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-muted-foreground mb-1">图标</label>
              <div className="grid grid-cols-5 gap-2">
                {iconOptions.map(({ name, icon: Icon }) => (
                  <button
                    key={name}
                    type="button"
                    onClick={() => setFormData({ ...formData, icon: name })}
                    className={`flex flex-col items-center justify-center p-2 rounded-lg border-2 transition-all ${
                      formData.icon === name
                        ? 'border-primary bg-primary/10 text-primary'
                        : 'border-border hover:border-primary/50 hover:bg-muted'
                    }`}
                    title={name}
                  >
                    <Icon className="w-5 h-5" />
                    <span className="text-[10px] mt-1 truncate w-full text-center">{name}</span>
                  </button>
                ))}
              </div>
            </div>
            <div className="col-span-2">
              <label className="block text-xs font-medium text-muted-foreground mb-1">描述</label>
              <input
                type="text"
                value={formData.description}
                onChange={(e) => setFormData({ ...formData, description: e.target.value })}
                className="w-full px-3 py-2 text-sm rounded-lg border border-input bg-background"
                placeholder="简短描述"
              />
            </div>
            <div className="col-span-2">
              <label className="block text-xs font-medium text-muted-foreground mb-1">提示内容 *</label>
              <textarea
                value={formData.prompt}
                onChange={(e) => setFormData({ ...formData, prompt: e.target.value })}
                className="w-full px-3 py-2 text-sm rounded-lg border border-input bg-background resize-none"
                placeholder="用户点击后发送的消息内容"
                rows={3}
                required
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-muted-foreground mb-1">分类</label>
              <input
                type="text"
                value={formData.category}
                onChange={(e) => setFormData({ ...formData, category: e.target.value })}
                className="w-full px-3 py-2 text-sm rounded-lg border border-input bg-background"
                placeholder="可选"
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-muted-foreground mb-1">排序</label>
              <input
                type="number"
                value={formData.sort_order}
                onChange={(e) => setFormData({ ...formData, sort_order: parseInt(e.target.value) || 0 })}
                className="w-full px-3 py-2 text-sm rounded-lg border border-input bg-background"
                min="0"
              />
            </div>
          </div>
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={handleCancel}
              className="px-3 py-1.5 text-sm text-muted-foreground hover:text-foreground"
            >
              取消
            </button>
            <button
              type="submit"
              className="px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90"
            >
              {editingId ? '更新' : '创建'}
            </button>
          </div>
        </form>
      )}

      {/* 列表 */}
      {isLoading ? (
        <p className="text-center text-muted-foreground py-8">加载中...</p>
      ) : prompts.length === 0 ? (
        <p className="text-center text-muted-foreground py-8">暂无快捷提示</p>
      ) : (
        <div className="space-y-2">
          {prompts.map((p) => (
            <div key={p.id} className={`p-3 rounded-lg border border-border hover:bg-muted/50 ${!p.is_enabled ? 'opacity-60' : ''}`}>
              <div className="flex items-start justify-between">
                <div className="flex-1">
                  <div className="flex items-center gap-2">
                    <span className="text-lg">{renderIcon(p.icon, 'w-5 h-5')}</span>
                    <p className="font-medium text-foreground">{p.title}</p>
                    {!p.is_enabled && (
                      <span className="px-1.5 py-0.5 rounded text-xs bg-gray-500/20 text-gray-600">已禁用</span>
                    )}
                  </div>
                  {p.description && (
                    <p className="text-xs text-muted-foreground mt-1">{p.description}</p>
                  )}
                  <p className="text-sm text-muted-foreground mt-2 line-clamp-2">{p.prompt}</p>
                </div>
                <div className="flex items-center gap-1 ml-2">
                  <button
                    onClick={() => handleChangeSort(p.id, p.sort_order, -1)}
                    className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted rounded"
                    title="上移"
                  >
                    <ChevronUp className="h-4 w-4" />
                  </button>
                  <button
                    onClick={() => handleChangeSort(p.id, p.sort_order, 1)}
                    className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted rounded"
                    title="下移"
                  >
                    <ChevronDown className="h-4 w-4" />
                  </button>
                  <button
                    onClick={() => handleToggleEnabled(p.id, p.is_enabled)}
                    className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted rounded"
                    title={p.is_enabled ? '禁用' : '启用'}
                  >
                    {p.is_enabled ? (
                      <ToggleRight className="h-4 w-4 text-green-500" />
                    ) : (
                      <ToggleLeft className="h-4 w-4 text-gray-400" />
                    )}
                  </button>
                  <button
                    onClick={() => handleEdit(p)}
                    className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted rounded"
                    title="编辑"
                  >
                    <Edit2 className="h-4 w-4" />
                  </button>
                  <button
                    onClick={() => handleDelete(p.id, p.title)}
                    className="p-1.5 text-muted-foreground hover:text-destructive hover:bg-muted rounded"
                    title="删除"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* 删除确认对话框 */}
      <ConfirmDialog
        isOpen={!!showDeleteConfirm}
        onClose={cancelDelete}
        onConfirm={confirmDelete}
        title="确认删除"
        message={showDeleteConfirm ? `确定要删除快捷提示 "${showDeleteConfirm.title}" 吗？此操作无法撤销。` : ''}
        confirmText="删除"
        cancelText="取消"
        variant="destructive"
      />
    </div>
  )
}
