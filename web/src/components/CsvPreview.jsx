import { useEffect, useRef, useState } from 'react'
import Papa from 'papaparse'

function CsvPreview({ src, blob, arrayBuffer, filename = '', className = '' } = {}) {
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [rows, setRows] = useState([])
  const [headers, setHeaders] = useState([])
  const [useHeaderRow, setUseHeaderRow] = useState(true)
  const [totalRows, setTotalRows] = useState(0)
  const loadReqIdRef = useRef({ v: 0 })

  useEffect(() => {
    if (!src && !blob && !arrayBuffer) {
      setRows([])
      setHeaders([])
      setError('')
      setLoading(false)
      return
    }

    const rid = (loadReqIdRef.current.v += 1)
    setLoading(true)
    setError('')
    setRows([])
    setHeaders([])

    ;(async () => {
      try {
        let text = ''
        if (arrayBuffer) {
          text = new TextDecoder('utf-8').decode(arrayBuffer)
        } else if (blob) {
          text = await blob.text()
        } else {
          const token = localStorage.getItem('token')
          const headers = {}
          if (token) headers.Authorization = `Bearer ${token}`
          const resp = await fetch(String(src), { credentials: 'include', headers })
          if (!resp.ok) throw new Error(resp.statusText || '下载失败')
          text = await resp.text()
        }

        if (loadReqIdRef.current.v !== rid) return

        const result = Papa.parse(text, {
          header: useHeaderRow,
          skipEmptyLines: true,
          dynamicTyping: false,
        })

        if (loadReqIdRef.current.v !== rid) return

        if (useHeaderRow) {
          setHeaders(result.meta?.fields || [])
          setRows(result.data || [])
        } else {
          const data = result.data || []
          const maxCols = data.reduce((m, r) => Math.max(m, r.length), 0)
          setHeaders(Array.from({ length: maxCols }, (_, i) => String.fromCharCode(65 + i)))
          setRows(data)
        }
        setTotalRows((result.data || []).length)
      } catch (e) {
        if (loadReqIdRef.current.v !== rid) return
        setError(e?.message || String(e || '解析失败'))
      } finally {
        if (loadReqIdRef.current.v !== rid) return
        setLoading(false)
      }
    })()
  }, [src, blob, arrayBuffer, useHeaderRow])

  const title = filename || (src ? String(src).split('/').pop() || '' : '')
  const MAX_DISPLAY = 2000

  if (!src && !blob && !arrayBuffer) {
    return <div className={`text-sm text-muted-foreground ${className}`}>暂无预览内容</div>
  }

  return (
    <div className={`flex flex-col min-h-0 ${className}`}>
      {/* 工具栏 */}
      <div className="flex items-center justify-between gap-2 mb-2 flex-wrap">
        <div className="min-w-0">
          <div className="text-sm font-semibold truncate" title={title}>{title || 'CSV 文件'}</div>
          {!loading && rows.length > 0 && (
            <div className="text-xs text-muted-foreground">
              共 {totalRows} 行 × {headers.length} 列
              {totalRows > MAX_DISPLAY && <span className="text-warning ml-1">（仅显示前 {MAX_DISPLAY} 行）</span>}
            </div>
          )}
        </div>
        <label className="label cursor-pointer gap-2">
          <span className="label-text text-xs">首行表头</span>
          <input
            type="checkbox"
            className="toggle toggle-sm"
            checked={useHeaderRow}
            onChange={(e) => setUseHeaderRow(e.target.checked)}
          />
        </label>
      </div>

      {loading && (
        <div className="py-16 flex justify-center">
          <span className="loading loading-spinner loading-lg"></span>
        </div>
      )}
      {error && (
        <div className="text-sm text-destructive break-words px-2">{error}</div>
      )}
      {!loading && !error && rows.length === 0 && (
        <div className="text-sm text-muted-foreground text-center py-8">无数据</div>
      )}
      {!loading && !error && rows.length > 0 && (
        <div className="flex-1 min-h-0 border border-border rounded bg-background overflow-auto">
          <table className="table table-xs table-pin-rows w-full">
            <thead>
              <tr className="bg-base-200">
                <th className="w-10 text-center text-muted-foreground font-normal">#</th>
                {headers.map((h, i) => (
                  <th key={i} className="whitespace-nowrap font-semibold" title={h}>{h || `列${i + 1}`}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.slice(0, MAX_DISPLAY).map((row, ri) => (
                <tr key={ri} className="hover:bg-base-100">
                  <td className="text-center text-muted-foreground text-xs">{ri + 1}</td>
                  {headers.map((h, ci) => {
                    const val = useHeaderRow ? (row[h] ?? '') : (row[ci] ?? '')
                    return (
                      <td key={ci} className="max-w-[200px] truncate" title={String(val)}>{String(val)}</td>
                    )
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}

export default CsvPreview
