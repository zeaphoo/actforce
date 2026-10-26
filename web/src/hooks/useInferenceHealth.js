import { useState, useEffect, useRef, useCallback } from 'react'
import { getInferenceHealth } from '../lib/api'

const POLL_INTERVAL = 5000

export function useInferenceHealth() {
  const [isReady, setIsReady] = useState(true) // 默认 true 避免闪烁
  const [processState, setProcessState] = useState('stopped') // stopped/starting/warming_up/running/restarting/failed
  const [message, setMessage] = useState('')
  const [checking, setChecking] = useState(false)
  const timerRef = useRef(null)
  const mountedRef = useRef(true)

  const check = useCallback(async () => {
    setChecking(true)
    try {
      const data = await getInferenceHealth()
      if (!mountedRef.current) return
      setIsReady(data.is_ready || data.ready)
      setProcessState(data.process_state || 'stopped')
      setMessage(data.message || '')
      return data
    } catch {
      if (!mountedRef.current) return
      setIsReady(false)
      setMessage('无法连接到推理引擎')
    } finally {
      if (mountedRef.current) setChecking(false)
    }
  }, [])

  useEffect(() => {
    mountedRef.current = true

    // 首次检查
    check()

    // 轮询
    const poll = async () => {
      const data = await check()
      if (!mountedRef.current) return
      if (!(data?.is_ready || data?.ready)) {
        timerRef.current = setTimeout(poll, POLL_INTERVAL)
      }
    }

    timerRef.current = setTimeout(poll, POLL_INTERVAL)

    return () => {
      mountedRef.current = false
      clearTimeout(timerRef.current)
    }
  }, [check])

  return { isReady, processState, message, checking, recheck: check }
}
