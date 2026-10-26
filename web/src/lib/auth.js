/**
 * Auth API and state management
 */

const AUTH_API_URL = '/api/auth'

// Storage keys
const IDENTITY_KEY = 'actforce-identity'
const TOKEN_KEY = 'actforce-token'
const REFRESH_TOKEN_KEY = 'actforce-refresh-token'
const USER_KEY = 'actforce-user'

// ============== Storage Helpers ==============

export function getStoredIdentity() {
  return localStorage.getItem(IDENTITY_KEY)
}

export function setStoredIdentity(identityId) {
  localStorage.setItem(IDENTITY_KEY, identityId)
}

export function getStoredToken() {
  return localStorage.getItem(TOKEN_KEY)
}

export function setStoredToken(token) {
  if (token) {
    localStorage.setItem(TOKEN_KEY, token)
  } else {
    localStorage.removeItem(TOKEN_KEY)
  }
}

export function getStoredRefreshToken() {
  return localStorage.getItem(REFRESH_TOKEN_KEY)
}

export function setStoredRefreshToken(token) {
  if (token) {
    localStorage.setItem(REFRESH_TOKEN_KEY, token)
  } else {
    localStorage.removeItem(REFRESH_TOKEN_KEY)
  }
}

export function getStoredUser() {
  try {
    const data = localStorage.getItem(USER_KEY)
    return data ? JSON.parse(data) : null
  } catch {
    return null
  }
}

export function setStoredUser(user) {
  if (user) {
    localStorage.setItem(USER_KEY, JSON.stringify(user))
  } else {
    localStorage.removeItem(USER_KEY)
  }
}

export function clearAuthStorage() {
  localStorage.removeItem(TOKEN_KEY)
  localStorage.removeItem(REFRESH_TOKEN_KEY)
  localStorage.removeItem(USER_KEY)
  // Note: Keep identity for anonymous usage
}

// ============== Auth API ==============

/**
 * Initialize or get existing identity
 * Called on app startup
 */
export async function initIdentity() {
  const existingIdentity = getStoredIdentity()
  
  // 先验证已有 identity 是否仍然有效
  if (existingIdentity) {
    try {
      const response = await fetch(`${AUTH_API_URL}/identity/init`)
      if (response.ok) {
        return { identity_id: existingIdentity }
      }
      // 403 表示匿名被禁用，清除缓存并拒绝
      if (response.status === 403) {
        localStorage.removeItem(IDENTITY_KEY)
        throw new Error('匿名使用已被禁用')
      }
    } catch (e) {
      if (e.message === '匿名使用已被禁用') throw e
      // 网络错误，用缓存的
      return { identity_id: existingIdentity }
    }
  }
  
  // 请求新的 identity
  const response = await fetch(`${AUTH_API_URL}/identity/init`)
  
  if (!response.ok) {
    throw new Error('Failed to initialize identity')
  }
  
  const data = await response.json()
  setStoredIdentity(data.identity_id)
  
  return data
}

/**
 * Login with username and password
 * @param {string} loginId - Username/login ID
 * @param {string} password - Password (will be MD5 hashed client-side)
 */
export async function login(loginId, password) {
  // MD5 hash the password client-side (matching Python implementation)
  const passwordMd5 = await md5Hash(password)
  
  const identityId = getStoredIdentity()
  
  const response = await fetch(`${AUTH_API_URL}/login`, {
    method: 'POST',
    headers: { 
      'Content-Type': 'application/json',
      ...(identityId && { 'X-Identity-Id': identityId }),
    },
    body: JSON.stringify({ 
      login_id: loginId,
      password_md5: passwordMd5,
    }),
  })
  
  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    throw new Error(error.detail || 'Login failed')
  }
  
  const data = await response.json()
  
  // Store tokens and user info
  setStoredToken(data.access_token)
  setStoredRefreshToken(data.refresh_token)
  setStoredUser(data.user)
  
  // Update identity if changed
  if (data.identity_id) {
    setStoredIdentity(data.identity_id)
  }
  
  return data
}

/**
 * Logout current user
 */
export async function logout() {
  const token = getStoredToken()
  const identityId = getStoredIdentity()
  
  try {
    await fetch(`${AUTH_API_URL}/logout`, {
      method: 'POST',
      headers: { 
        'Content-Type': 'application/json',
        ...(token && { 'Authorization': `Bearer ${token}` }),
        ...(identityId && { 'X-Identity-Id': identityId }),
      },
    })
  } catch (e) {
    console.warn('Logout request failed:', e)
  }
  
  clearAuthStorage()
}

/**
 * Refresh access token
 */
export async function refreshToken() {
  const refreshTokenValue = getStoredRefreshToken()
  const identityId = getStoredIdentity()
  
  if (!refreshTokenValue) {
    throw new Error('No refresh token')
  }
  
  const response = await fetch(`${AUTH_API_URL}/refresh`, {
    method: 'POST',
    headers: { 
      'Content-Type': 'application/json',
      ...(identityId && { 'X-Identity-Id': identityId }),
    },
    body: JSON.stringify({ 
      refresh_token: refreshTokenValue,
    }),
  })
  
  if (!response.ok) {
    clearAuthStorage()
    throw new Error('Token refresh failed')
  }
  
  const data = await response.json()
  setStoredToken(data.access_token)
  
  return data
}

/**
 * Get current user info
 */
export async function getCurrentUser() {
  const token = getStoredToken()
  const identityId = getStoredIdentity()
  
  if (!token) {
    return null
  }
  
  const response = await fetch(`${AUTH_API_URL}/me`, {
    headers: { 
      'Authorization': `Bearer ${token}`,
      ...(identityId && { 'X-Identity-Id': identityId }),
    },
  })
  
  if (!response.ok) {
    if (response.status === 401) {
      // Try to refresh token
      try {
        await refreshToken()
        return getCurrentUser()
      } catch {
        clearAuthStorage()
        return null
      }
    }
    throw new Error('Failed to get user info')
  }
  
  const data = await response.json()
  setStoredUser(data.user)
  
  return data
}

// ============== Helpers ==============

/**
 * Get auth headers for API requests
 */
export function getAuthHeaders() {
  const token = getStoredToken()
  const identityId = getStoredIdentity()
  
  return {
    ...(token && { 'Authorization': `Bearer ${token}` }),
    ...(identityId && { 'X-Identity-Id': identityId }),
  }
}

/**
 * 处理 API 响应，自动处理 401 错误
 * @param {Response} response - fetch 响应对象
 * @param {Function} onUnauthorized - 401 时的回调函数
 * @returns {Promise<any>} 解析后的响应数据
 */
export async function handleApiResponse(response, onUnauthorized) {
  if (!response.ok) {
    if (response.status === 401) {
      // Token 过期或无效
      if (onUnauthorized) {
        onUnauthorized()
      }
      throw new Error('会话已过期，请重新登录')
    }
    
    const error = await response.json().catch(() => ({}))
    throw new Error(error.detail || `请求失败: ${response.status}`)
  }
  
  return response.json()
}


/**
 * Check if user is logged in
 */
export function isLoggedIn() {
  return !!getStoredToken() && !!getStoredUser()
}

/**
 * MD5 hash function (using SubtleCrypto)
 */
async function md5Hash(message) {
  // Use a simple MD5 implementation since SubtleCrypto doesn't support MD5
  // This matches the Python client-side MD5 hashing
  return md5(message)
}

// Simple MD5 implementation
export function md5(string) {
  function rotateLeft(value, shift) {
    return (value << shift) | (value >>> (32 - shift))
  }

  function addUnsigned(x, y) {
    const x8 = x & 0x80000000
    const y8 = y & 0x80000000
    const x4 = x & 0x40000000
    const y4 = y & 0x40000000
    const result = (x & 0x3fffffff) + (y & 0x3fffffff)
    if (x4 & y4) return result ^ 0x80000000 ^ x8 ^ y8
    if (x4 | y4) {
      if (result & 0x40000000) return result ^ 0xc0000000 ^ x8 ^ y8
      return result ^ 0x40000000 ^ x8 ^ y8
    }
    return result ^ x8 ^ y8
  }

  function f(x, y, z) { return (x & y) | (~x & z) }
  function g(x, y, z) { return (x & z) | (y & ~z) }
  function h(x, y, z) { return x ^ y ^ z }
  function i(x, y, z) { return y ^ (x | ~z) }

  function ff(a, b, c, d, x, s, ac) {
    a = addUnsigned(a, addUnsigned(addUnsigned(f(b, c, d), x), ac))
    return addUnsigned(rotateLeft(a, s), b)
  }

  function gg(a, b, c, d, x, s, ac) {
    a = addUnsigned(a, addUnsigned(addUnsigned(g(b, c, d), x), ac))
    return addUnsigned(rotateLeft(a, s), b)
  }

  function hh(a, b, c, d, x, s, ac) {
    a = addUnsigned(a, addUnsigned(addUnsigned(h(b, c, d), x), ac))
    return addUnsigned(rotateLeft(a, s), b)
  }

  function ii(a, b, c, d, x, s, ac) {
    a = addUnsigned(a, addUnsigned(addUnsigned(i(b, c, d), x), ac))
    return addUnsigned(rotateLeft(a, s), b)
  }

  function convertToWordArray(string) {
    let messageLength = string.length
    let numberOfWords = (((messageLength + 8) - ((messageLength + 8) % 64)) / 64 + 1) * 16
    let wordArray = new Array(numberOfWords - 1)
    let bytePosition = 0
    let byteCount = 0
    while (byteCount < messageLength) {
      let wordCount = (byteCount - (byteCount % 4)) / 4
      bytePosition = (byteCount % 4) * 8
      wordArray[wordCount] = (wordArray[wordCount] | (string.charCodeAt(byteCount) << bytePosition))
      byteCount++
    }
    let wordCount = (byteCount - (byteCount % 4)) / 4
    bytePosition = (byteCount % 4) * 8
    wordArray[wordCount] = wordArray[wordCount] | (0x80 << bytePosition)
    wordArray[numberOfWords - 2] = messageLength << 3
    wordArray[numberOfWords - 1] = messageLength >>> 29
    return wordArray
  }

  function wordToHex(value) {
    let hex = ''
    for (let i = 0; i <= 3; i++) {
      let byte = (value >>> (i * 8)) & 255
      hex += ('0' + byte.toString(16)).slice(-2)
    }
    return hex
  }

  let x = convertToWordArray(string)
  let a = 0x67452301, b = 0xefcdab89, c = 0x98badcfe, d = 0x10325476

  const S11 = 7, S12 = 12, S13 = 17, S14 = 22
  const S21 = 5, S22 = 9, S23 = 14, S24 = 20
  const S31 = 4, S32 = 11, S33 = 16, S34 = 23
  const S41 = 6, S42 = 10, S43 = 15, S44 = 21

  for (let k = 0; k < x.length; k += 16) {
    let AA = a, BB = b, CC = c, DD = d
    a = ff(a, b, c, d, x[k + 0], S11, 0xd76aa478)
    d = ff(d, a, b, c, x[k + 1], S12, 0xe8c7b756)
    c = ff(c, d, a, b, x[k + 2], S13, 0x242070db)
    b = ff(b, c, d, a, x[k + 3], S14, 0xc1bdceee)
    a = ff(a, b, c, d, x[k + 4], S11, 0xf57c0faf)
    d = ff(d, a, b, c, x[k + 5], S12, 0x4787c62a)
    c = ff(c, d, a, b, x[k + 6], S13, 0xa8304613)
    b = ff(b, c, d, a, x[k + 7], S14, 0xfd469501)
    a = ff(a, b, c, d, x[k + 8], S11, 0x698098d8)
    d = ff(d, a, b, c, x[k + 9], S12, 0x8b44f7af)
    c = ff(c, d, a, b, x[k + 10], S13, 0xffff5bb1)
    b = ff(b, c, d, a, x[k + 11], S14, 0x895cd7be)
    a = ff(a, b, c, d, x[k + 12], S11, 0x6b901122)
    d = ff(d, a, b, c, x[k + 13], S12, 0xfd987193)
    c = ff(c, d, a, b, x[k + 14], S13, 0xa679438e)
    b = ff(b, c, d, a, x[k + 15], S14, 0x49b40821)
    a = gg(a, b, c, d, x[k + 1], S21, 0xf61e2562)
    d = gg(d, a, b, c, x[k + 6], S22, 0xc040b340)
    c = gg(c, d, a, b, x[k + 11], S23, 0x265e5a51)
    b = gg(b, c, d, a, x[k + 0], S24, 0xe9b6c7aa)
    a = gg(a, b, c, d, x[k + 5], S21, 0xd62f105d)
    d = gg(d, a, b, c, x[k + 10], S22, 0x2441453)
    c = gg(c, d, a, b, x[k + 15], S23, 0xd8a1e681)
    b = gg(b, c, d, a, x[k + 4], S24, 0xe7d3fbc8)
    a = gg(a, b, c, d, x[k + 9], S21, 0x21e1cde6)
    d = gg(d, a, b, c, x[k + 14], S22, 0xc33707d6)
    c = gg(c, d, a, b, x[k + 3], S23, 0xf4d50d87)
    b = gg(b, c, d, a, x[k + 8], S24, 0x455a14ed)
    a = gg(a, b, c, d, x[k + 13], S21, 0xa9e3e905)
    d = gg(d, a, b, c, x[k + 2], S22, 0xfcefa3f8)
    c = gg(c, d, a, b, x[k + 7], S23, 0x676f02d9)
    b = gg(b, c, d, a, x[k + 12], S24, 0x8d2a4c8a)
    a = hh(a, b, c, d, x[k + 5], S31, 0xfffa3942)
    d = hh(d, a, b, c, x[k + 8], S32, 0x8771f681)
    c = hh(c, d, a, b, x[k + 11], S33, 0x6d9d6122)
    b = hh(b, c, d, a, x[k + 14], S34, 0xfde5380c)
    a = hh(a, b, c, d, x[k + 1], S31, 0xa4beea44)
    d = hh(d, a, b, c, x[k + 4], S32, 0x4bdecfa9)
    c = hh(c, d, a, b, x[k + 7], S33, 0xf6bb4b60)
    b = hh(b, c, d, a, x[k + 10], S34, 0xbebfbc70)
    a = hh(a, b, c, d, x[k + 13], S31, 0x289b7ec6)
    d = hh(d, a, b, c, x[k + 0], S32, 0xeaa127fa)
    c = hh(c, d, a, b, x[k + 3], S33, 0xd4ef3085)
    b = hh(b, c, d, a, x[k + 6], S34, 0x4881d05)
    a = hh(a, b, c, d, x[k + 9], S31, 0xd9d4d039)
    d = hh(d, a, b, c, x[k + 12], S32, 0xe6db99e5)
    c = hh(c, d, a, b, x[k + 15], S33, 0x1fa27cf8)
    b = hh(b, c, d, a, x[k + 2], S34, 0xc4ac5665)
    a = ii(a, b, c, d, x[k + 0], S41, 0xf4292244)
    d = ii(d, a, b, c, x[k + 7], S42, 0x432aff97)
    c = ii(c, d, a, b, x[k + 14], S43, 0xab9423a7)
    b = ii(b, c, d, a, x[k + 5], S44, 0xfc93a039)
    a = ii(a, b, c, d, x[k + 12], S41, 0x655b59c3)
    d = ii(d, a, b, c, x[k + 3], S42, 0x8f0ccc92)
    c = ii(c, d, a, b, x[k + 10], S43, 0xffeff47d)
    b = ii(b, c, d, a, x[k + 1], S44, 0x85845dd1)
    a = ii(a, b, c, d, x[k + 8], S41, 0x6fa87e4f)
    d = ii(d, a, b, c, x[k + 15], S42, 0xfe2ce6e0)
    c = ii(c, d, a, b, x[k + 6], S43, 0xa3014314)
    b = ii(b, c, d, a, x[k + 13], S44, 0x4e0811a1)
    a = ii(a, b, c, d, x[k + 4], S41, 0xf7537e82)
    d = ii(d, a, b, c, x[k + 11], S42, 0xbd3af235)
    c = ii(c, d, a, b, x[k + 2], S43, 0x2ad7d2bb)
    b = ii(b, c, d, a, x[k + 9], S44, 0xeb86d391)
    a = addUnsigned(a, AA)
    b = addUnsigned(b, BB)
    c = addUnsigned(c, CC)
    d = addUnsigned(d, DD)
  }

  return (wordToHex(a) + wordToHex(b) + wordToHex(c) + wordToHex(d)).toLowerCase()
}
