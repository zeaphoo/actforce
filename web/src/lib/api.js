import { getAuthHeaders, getStoredIdentity } from './auth'

const API_BASE_URL = '/api/chat'

// 获取所有对话列表
export async function getConversations() {
  const identityId = getStoredIdentity()
  if (!identityId) {
    return []
  }
  const response = await fetch(`${API_BASE_URL}/conversations?identity_id=${encodeURIComponent(identityId)}`, {
    headers: {
      ...getAuthHeaders(),
    },
  })
  if (!response.ok) {
    throw new Error('Failed to fetch conversations')
  }
  return await response.json()
}

// 创建新对话
export async function createConversation(title = null) {
  const identityId = getStoredIdentity()
  if (!identityId) {
    throw new Error('No identity found')
  }
  const response = await fetch(`${API_BASE_URL}/conversations?identity_id=${encodeURIComponent(identityId)}`, {
    method: 'POST',
    headers: { 
      'Content-Type': 'application/json',
      ...getAuthHeaders(),
    },
    body: JSON.stringify({ title }),
  })
  if (!response.ok) {
    throw new Error('Failed to create conversation')
  }
  return await response.json()
}

// 获取指定对话的消息
export async function getThreadMessages(conversationId) {
  if (!conversationId) return []
  const identityId = getStoredIdentity()
  if (!identityId) {
    return []
  }
  const response = await fetch(`${API_BASE_URL}/conversations/${conversationId}/messages?identity_id=${encodeURIComponent(identityId)}`, {
    headers: {
      ...getAuthHeaders(),
    },
  })
  if (!response.ok) {
    throw new Error('Failed to fetch messages')
  }
  return await response.json()
}

// 发送消息到后端 API
export async function sendMessage(message, threadId, agentId = null) {
  const response = await fetch(`${API_BASE_URL}/`, {
    method: 'POST',
    headers: { 
      'Content-Type': 'application/json',
      ...getAuthHeaders(),
    },
    body: JSON.stringify({ 
      message, 
      conversation_id: threadId,
      thread_id: threadId,
      agent_id: agentId,
    }),
  })

  if (!response.ok) {
    throw new Error('Failed to send message')
  }

  return await response.json()
}

// 保存对话消息到后端 (实际是发送消息并获取回复)
export async function saveThreadMessagesForThread(threadId, messages, agentId = null) {
  const lastMessage = messages[messages.length - 1]
  if (lastMessage && lastMessage.role === 'user') {
    await sendMessage(lastMessage.content[0]?.text || '', threadId, agentId)
  }
}

// 删除对话的消息 (调用后端删除对话)
export async function deleteThreadMessages(threadId) {
  const identityId = getStoredIdentity()
  if (!identityId) {
    throw new Error('No identity found')
  }
  const response = await fetch(`${API_BASE_URL}/conversations/${threadId}?identity_id=${encodeURIComponent(identityId)}`, {
    method: 'DELETE',
    headers: {
      ...getAuthHeaders(),
    },
  })
  if (!response.ok) {
    throw new Error('Failed to delete conversation')
  }
  return await response.json()
}

// 检查推理引擎健康状态
export async function getInferenceHealth() {
  const response = await fetch('/api/inference/health', {
    headers: getAuthHeaders(),
  })
  if (!response.ok) {
    throw new Error(`Failed to check inference health: ${response.status}`)
  }
  return await response.json()
}

// 更新对话标题
export async function updateConversationTitle(threadId, title) {
  const identityId = getStoredIdentity()
  if (!identityId) {
    throw new Error('No identity found')
  }
  const response = await fetch(`${API_BASE_URL}/conversations/${threadId}?identity_id=${encodeURIComponent(identityId)}&title=${encodeURIComponent(title)}`, {
    method: 'PATCH',
    headers: {
      ...getAuthHeaders(),
    },
  })
  if (!response.ok) {
    throw new Error('Failed to update conversation title')
  }
  return await response.json()
}
