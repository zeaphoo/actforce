import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import * as XLSX from 'xlsx'
import DataEditor, { GridCellKind } from '@glideapps/glide-data-grid'
import '@glideapps/glide-data-grid/dist/index.css'

function ExcelPreview({ src, blob, arrayBuffer, filename = '', className = '' } = {}) {
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [workbook, setWorkbook] = useState(null)
  const [sheetName, setSheetName] = useState('')
  const [useHeaderRow, setUseHeaderRow] = useState(true)

  const [sheetState, setSheetState] = useState(() => ({ ws: null, range: null, rowOffset: 0, rows: 0, cols: 0 }))
  const [columnWidths, setColumnWidths] = useState({})

  const loadReqIdRef = useRef({ v: 0 })

  useEffect(() => {
    if (!src && !blob && !arrayBuffer) {
      setWorkbook(null)
      setSheetName('')
      setSheetState({ ws: null, range: null, rowOffset: 0, rows: 0, cols: 0 })
      setError('')
      setLoading(false)
      return
    }

    const rid = (loadReqIdRef.current.v += 1)
    setLoading(true)
    setError('')
    setWorkbook(null)
    setSheetName('')
    setSheetState({ ws: null, range: null, rowOffset: 0, rows: 0, cols: 0 })

    ;(async () => {
      try {
        let ab = arrayBuffer
        if (!ab && blob) {
          ab = await blob.arrayBuffer()
        }
        if (!ab) {
          const token = localStorage.getItem('token')
          const headers = {}
          if (token) headers.Authorization = `Bearer ${token}`
          const resp = await fetch(String(src), {
            credentials: 'include',
            headers,
          })
          if (!resp.ok) throw new Error(resp.statusText || '下载失败')
          ab = await resp.arrayBuffer()
        }
        if (loadReqIdRef.current.v !== rid) return

        const wb = XLSX.read(ab, { type: 'array' })
        const names = Array.isArray(wb?.SheetNames) ? wb.SheetNames : []
        const first = names[0] || ''

        setWorkbook(wb)
        setSheetName(first)
      } catch (e) {
        if (loadReqIdRef.current.v !== rid) return
        setError(e?.message || String(e || '解析失败'))
        setWorkbook(null)
        setSheetName('')
      } finally {
        if (loadReqIdRef.current.v !== rid) return
        setLoading(false)
      }
    })()
  }, [src, blob, arrayBuffer])

  useEffect(() => {
    const wb = workbook
    if (!wb || !sheetName) {
      setSheetState({ ws: null, range: null, rowOffset: 0, rows: 0, cols: 0 })
      return
    }

    const ws = wb.Sheets?.[sheetName]
    const ref = ws?.['!ref']
    if (!ws || !ref) {
      setSheetState({ ws: null, range: null, rowOffset: 0, rows: 0, cols: 0 })
      return
    }

    const range = XLSX.utils.decode_range(ref)
    const totalRows = Math.max(0, range.e.r - range.s.r + 1)
    const totalCols = Math.max(0, range.e.c - range.s.c + 1)

    const rowOffset = useHeaderRow ? 1 : 0
    const rows = Math.max(0, totalRows - rowOffset)

    setSheetState({ ws, range, rowOffset, rows, cols: totalCols })
    setColumnWidths({})
  }, [workbook, sheetName, useHeaderRow])

  const sheetNames = useMemo(() => {
    const wb = workbook
    const names = Array.isArray(wb?.SheetNames) ? wb.SheetNames : []
    return names.map((n) => String(n || ''))
  }, [workbook])

  const columns = useMemo(() => {
    const { ws, range, cols } = sheetState
    if (!ws || !range || cols <= 0) return []

    const makeTitle = (c) => {
      if (!useHeaderRow) return XLSX.utils.encode_col(c)
      const addr = XLSX.utils.encode_cell({ c: range.s.c + c, r: range.s.r })
      const cell = ws?.[addr]
      const v = cell?.w ?? cell?.v
      const s = v === undefined || v === null ? '' : String(v)
      return s || XLSX.utils.encode_col(c)
    }

    const result = []
    for (let c = 0; c < cols; c += 1) {
      result.push({ id: String(c), title: makeTitle(c), width: columnWidths[c] ?? 140 })
    }
    return result
  }, [sheetState, useHeaderRow, columnWidths])

  const getCellContent = useCallback(
    ([col, row]) => {
      const { ws, range, rowOffset } = sheetState
      if (!ws || !range) {
        return {
          kind: GridCellKind.Text,
          data: '',
          displayData: '',
          allowOverlay: true,
        }
      }

      const addr = XLSX.utils.encode_cell({ c: range.s.c + col, r: range.s.r + rowOffset + row })
      const cell = ws?.[addr]
      const v = cell?.w ?? cell?.v
      const s = v === undefined || v === null ? '' : String(v)

      return {
        kind: GridCellKind.Text,
        data: s,
        displayData: s,
        allowOverlay: true,
      }
    },
    [sheetState],
  )

  const handleColumnResize = useCallback((column, newSize) => {
    const idx = Number(column.id)
    if (!Number.isNaN(idx)) {
      setColumnWidths(prev => ({ ...prev, [idx]: newSize }))
    }
  }, [])

  const title = useMemo(() => {
    const n = String(filename || '').trim()
    if (n) return n
    const u = String(src || '')
    const last = u.split('/').pop() || ''
    return last
  }, [filename, src])

  if (!src && !blob && !arrayBuffer) {
    return <div className={`text-sm text-muted-foreground ${className}`}>暂无预览内容</div>
  }

  if (loading) {
    return (
      <div className={`py-16 flex justify-center ${className}`}>
        <span className="loading loading-spinner loading-lg"></span>
      </div>
    )
  }

  if (error) {
    return <div className={`text-sm text-destructive break-words ${className}`}>{error}</div>
  }

  if (!workbook || !sheetName) {
    return <div className={`text-sm text-muted-foreground ${className}`}>暂无可预览的工作表</div>
  }

  return (
    <div className={`flex flex-col min-h-0 w-full ${className}`}>
      <div className="flex items-center justify-between gap-2 mb-2 flex-wrap">
        <div className="min-w-0">
          <div className="text-sm font-semibold truncate" title={title}>
            {title || 'Excel'}
          </div>
          <div className="text-xs text-muted-foreground truncate">{sheetName}</div>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          <select
            className="select select-bordered select-sm"
            value={sheetName}
            onChange={(e) => setSheetName(e.target.value)}
          >
            {sheetNames.map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>

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
      </div>

      <div className="flex-1 min-h-0 w-full border border-border rounded bg-background overflow-hidden">
        <DataEditor
          className="h-full w-full"
          columns={columns}
          rows={sheetState.rows}
          getCellContent={getCellContent}
          rowMarkers="number"
          smoothScrollX
          smoothScrollY
          onColumnResize={handleColumnResize}
        />
      </div>
    </div>
  )
}

export default ExcelPreview
