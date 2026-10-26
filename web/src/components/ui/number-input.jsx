import * as React from "react"
import { Slot } from "@radix-ui/react-slot"

/**
 * NumberInput 组件
 * 基于 shadcn/ui Input 封装，只允许输入数字，支持 min/max 区间限制。
 *
 * 内部使用 string 存储中间状态（如空字符串、"12"），
 * 只有在 onBlur 或输入完整数字后才应用 min/max 约束。
 */
export function NumberInput({
  value,
  onChange,
  min,
  max,
  className = "",
  asChild = false,
  ...props
}) {
  const Comp = asChild ? Slot : "input"

  // 将外部 value 转为显示字符串
  const displayValue = React.useMemo(() => {
    if (value === "" || value === undefined || value === null) return ""
    return String(value)
  }, [value])

  const [innerValue, setInnerValue] = React.useState(displayValue)

  // 同步外部 value 变化
  React.useEffect(() => {
    setInnerValue(displayValue)
  }, [displayValue])

  const handleChange = (e) => {
    const raw = e.target.value
    // 只保留数字和负号（负号只能在开头）
    const cleaned = raw.replace(/(?!^)-/g, "").replace(/[^0-9-]/g, "")
    setInnerValue(cleaned)

    // 空值或负号时通知外部为空
    if (cleaned === "" || cleaned === "-") {
      onChange("")
      return
    }

    const num = parseInt(cleaned, 10)
    if (isNaN(num)) return
    onChange(num)
  }

  const handleBlur = () => {
    // 失焦时应用 min/max 约束
    if (innerValue === "" || innerValue === "-") {
      // 空值时恢复为 min 或默认值
      const fallback = min !== undefined ? min : 1
      setInnerValue(String(fallback))
      onChange(fallback)
      return
    }

    let num = parseInt(innerValue, 10)
    if (isNaN(num)) {
      const fallback = min !== undefined ? min : 1
      setInnerValue(String(fallback))
      onChange(fallback)
      return
    }

    let clamped = num
    if (min !== undefined && clamped < min) clamped = min
    if (max !== undefined && clamped > max) clamped = max
    if (clamped !== num) {
      setInnerValue(String(clamped))
      onChange(clamped)
    }
  }

  const handleKeyDown = (e) => {
    // 阻止非数字按键（除了退格、删除、方向键、Tab、Home、End）
    const allowed = ["Backspace", "Delete", "ArrowLeft", "ArrowRight", "Tab", "Home", "End"]
    if (allowed.includes(e.key)) return
    if (e.ctrlKey || e.metaKey) return // 允许 Ctrl+A/C/V/X/Z/Y
    if (e.key === "-") {
      // 只允许在开头输入负号
      const input = e.target
      if (input.selectionStart !== 0) {
        e.preventDefault()
      }
      return
    }
    if (!/^[0-9]$/.test(e.key)) {
      e.preventDefault()
    }
  }

  const handlePaste = (e) => {
    // 过滤粘贴内容中的非数字字符
    const paste = (e.clipboardData || window.clipboardData).getData("text")
    const cleaned = paste.replace(/[^0-9]/g, "")
    if (cleaned) {
      e.preventDefault()
      const input = e.target
      const start = input.selectionStart ?? 0
      const end = input.selectionEnd ?? 0
      const newValue = innerValue.slice(0, start) + cleaned + innerValue.slice(end)
      setInnerValue(newValue)
      const num = parseInt(newValue, 10)
      if (!isNaN(num)) onChange(num)
    } else {
      e.preventDefault()
    }
  }

  return (
    <Comp
      type="text"
      inputMode="numeric"
      pattern="[0-9]*"
      value={innerValue}
      onChange={handleChange}
      onBlur={handleBlur}
      onKeyDown={handleKeyDown}
      onPaste={handlePaste}
      className={`w-full px-3 py-2 rounded-md border border-input bg-background text-sm ring-offset-background focus:outline-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 ${className}`}
      {...props}
    />
  )
}
