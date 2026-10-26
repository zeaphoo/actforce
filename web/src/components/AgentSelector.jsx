import { useState, useEffect } from 'react'
import { ChevronDown, Bot } from 'lucide-react'

const STORAGE_KEY = 'actforce-agents'
const SELECTED_AGENT_KEY = 'actforce-selected-agent'

// 模拟 Agent 列表（后续可以从后端 API 获取）
const DEFAULT_AGENTS = [
  { id: 'default', name: '通用助手', description: '智能对话助手' },
  { id: 'code-assistant', name: '代码助手', description: '编程和代码审查' },
  { id: 'data-analyst', name: '数据分析师', description: '数据分析和可视化' },
  { id: 'writer', name: '写作助手', description: '文案和创意写作' },
]

// 从 localStorage 加载 agents
function loadAgents() {
  try {
    const data = localStorage.getItem(STORAGE_KEY)
    return data ? JSON.parse(data) : DEFAULT_AGENTS
  } catch {
    return DEFAULT_AGENTS
  }
}

// 保存 agents 到 localStorage
function saveAgents(agents) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(agents))
}

// 获取选中的 agent
function getSelectedAgent() {
  try {
    return localStorage.getItem(SELECTED_AGENT_KEY) || 'default'
  } catch {
    return 'default'
  }
}

// 保存选中的 agent
function saveSelectedAgent(agentId) {
  localStorage.setItem(SELECTED_AGENT_KEY, agentId)
}

export function AgentSelector({ selectedAgentId, onSelectAgent }) {
  const [agents] = useState(loadAgents)
  const [isOpen, setIsOpen] = useState(false)

  const selectedAgent = agents.find(a => a.id === selectedAgentId) || agents[0]

  // 点击外部关闭下拉框
  useEffect(() => {
    const handleClickOutside = (e) => {
      if (!e.target.closest('.agent-selector')) {
        setIsOpen(false)
      }
    }
    document.addEventListener('click', handleClickOutside)
    return () => document.removeEventListener('click', handleClickOutside)
  }, [])

  const handleSelect = (agentId) => {
    onSelectAgent(agentId)
    saveSelectedAgent(agentId)
    setIsOpen(false)
  }

  return (
    <div className="relative agent-selector">
      {/* 选择按钮 */}
      <button
        onClick={() => setIsOpen(!isOpen)}
        className={`flex items-center gap-2 px-3 py-1 rounded-lg hover:bg-muted transition-colors text-sm ${isOpen ? 'rounded-b-none' : ''}`}
      >
        <Bot className="h-4 w-4 text-muted-foreground" />
        <div className="text-left">
          <div className="font-medium text-foreground">{selectedAgent.name}</div>
        </div>
        <ChevronDown className={`h-4 w-4 text-muted-foreground transition-transform ${isOpen ? 'rotate-180' : ''}`} />
      </button>

      {/* 下拉菜单 */}
      {isOpen && (
        <div className="absolute top-full left-0 w-64 rounded-b-lg bg-card shadow-lg z-50 overflow-hidden border border-border">
          <div className="py-1">
            {agents.map((agent) => (
              <button
                key={agent.id}
                onClick={() => handleSelect(agent.id)}
                className={`
                  w-full flex items-start gap-3 px-3 py-2.5 hover:bg-muted transition-colors text-left
                  ${agent.id === selectedAgentId ? 'bg-muted' : ''}
                `}
              >
                <Bot className="h-4 w-4 text-muted-foreground mt-0.5 flex-shrink-0" />
                <div className="flex-1 min-w-0">
                  <div className="font-medium text-foreground text-sm">{agent.name}</div>
                  <div className="text-xs text-muted-foreground truncate">{agent.description}</div>
                </div>
                {agent.id === selectedAgentId && (
                  <div className="w-2 h-2 rounded-full bg-primary mt-1.5 flex-shrink-0" />
                )}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

// Agent 管理 Hook
export function useAgentManager() {
  const [selectedAgentId, setSelectedAgentId] = useState(getSelectedAgent)

  return {
    selectedAgentId,
    setSelectedAgent: setSelectedAgentId,
  }
}
