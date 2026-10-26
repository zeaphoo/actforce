import { useEntitlements } from '../lib/EntitlementsContext'

/**
 * Generic feature/status gate. The server remains authoritative; this only
 * controls what the UI renders.
 */
export function AccessGate({ feature, fallback = null, children }) {
  const { hasFeature, isBlocked } = useEntitlements()

  if (isBlocked()) return fallback
  if (feature && !hasFeature(feature)) return fallback
  return children
}

export default AccessGate
