import { useEffect, useRef, useState } from 'react'

/**
 * Gradually reveal `target` one chunk at a time. Useful for smoothing out
 * bursty streaming tokens so the UI animates at a steady character rate.
 *
 * Uses requestAnimationFrame for smooth frame-synced animation.
 *
 * @param {string} target
 * @param {{ charsPerSecond?: number, enabled?: boolean }} [opts]
 * @returns {string}
 */
export function useSmoothText(target, opts = {}) {
  const { charsPerSecond = 120, enabled = true } = opts
  const [displayed, setDisplayed] = useState(enabled ? '' : target)

  const targetRef = useRef(target)
  targetRef.current = target
  const charsRef = useRef(charsPerSecond)
  charsRef.current = charsPerSecond

  useEffect(() => {
    let raf = 0
    let lastTs = 0

    const tick = (ts) => {
      if (!lastTs) lastTs = ts
      const dt = ts - lastTs
      lastTs = ts

      setDisplayed((prev) => {
        const t = targetRef.current
        if (!t.startsWith(prev)) return t
        if (prev.length >= t.length) return prev
        const advance = Math.max(1, Math.round((dt * charsRef.current) / 1000))
        return t.slice(0, Math.min(t.length, prev.length + advance))
      })

      raf = requestAnimationFrame(tick)
    }

    raf = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(raf)
  }, [])

  return displayed
}
