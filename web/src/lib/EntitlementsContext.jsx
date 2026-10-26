import { createContext, useCallback, useContext, useEffect, useState } from 'react'

const EntitlementsContext = createContext(null)

const DEFAULT_ENTITLEMENTS = {
  tier: 'community',
  status: 'community',
  features: [],
  limits: {},
  expires_at: null,
  grace_until: null,
  messages: [],
}

const FAIL_CLOSED_ENTITLEMENTS = {
  ...DEFAULT_ENTITLEMENTS,
  tier: 'unknown',
  status: 'unlicensed',
  messages: ['无法获取授权状态'],
}

/**
 * Generic authorization state. Public ActForce talks to /api/entitlements;
 * private ActForce-Super returns the same shape from its license provider.
 *
 * `failOpen` defaults to true for the community build. Private products pass
 * false so a backend/network failure cannot silently unlock the product.
 */
export function EntitlementsProvider({ children, failOpen = true }) {
  const [entitlements, setEntitlements] = useState(null)
  const [isLoading, setIsLoading] = useState(true)
  const [error, setError] = useState(null)

  const refresh = useCallback(async () => {
    try {
      const response = await fetch('/api/entitlements')
      if (!response.ok) {
        throw new Error(`Failed to fetch entitlements: ${response.status}`)
      }
      const data = await response.json()
      setEntitlements(data)
      setError(null)
      return data
    } catch (e) {
      console.error('Failed to fetch entitlements:', e)
      setError(e.message)
      const fallback = failOpen
        ? DEFAULT_ENTITLEMENTS
        : { ...FAIL_CLOSED_ENTITLEMENTS, messages: [e.message] }
      setEntitlements(fallback)
      return fallback
    }
  }, [failOpen])

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      await refresh()
      if (!cancelled) setIsLoading(false)
    })()
    return () => {
      cancelled = true
    }
  }, [refresh])

  const current = entitlements || DEFAULT_ENTITLEMENTS
  const value = {
    entitlements: current,
    isLoading,
    error,
    refresh,
    hasFeature: (feature) => {
      if (current.status === 'community') return true
      return Array.isArray(current.features) && current.features.includes(feature)
    },
    isBlocked: () =>
      ['expired', 'invalid', 'unlicensed'].includes(current.status),
    getLimit: (key) => current.limits?.[key] ?? null,
  }

  return (
    <EntitlementsContext.Provider value={value}>
      {children}
    </EntitlementsContext.Provider>
  )
}

export function useEntitlements() {
  const context = useContext(EntitlementsContext)
  if (!context) {
    throw new Error('useEntitlements must be used within an EntitlementsProvider')
  }
  return context
}

export default EntitlementsContext
