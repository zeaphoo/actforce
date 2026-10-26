import React, { useMemo, useState, useRef, useEffect } from 'react'
import ReactECharts from 'echarts-for-react'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu'

/**
 * AuPageRenderer - Renders .aupage.json documents with ECharts support.
 * 
 * Props:
 *   - doc: AuPageDocument object (parsed JSON)
 *   - images: Map of chart key -> { url: string } for externally provided chart images
 *   - className: optional CSS class
 *   - onExport: optional callback for export actions { format: 'pdf' | 'docx' | 'html' }
 */
export default function AuPageRenderer({ doc, images = {}, className = '', onExport }) {
  if (!doc) return null

  const meta = doc.meta || {}
  const pageWidth = meta.page_width || 794
  const [sidebarOpen, setSidebarOpen] = useState(true)
  const [exporting, setExporting] = useState(false)
  const sections = useMemo(
    () => doc.elements.filter((e) => e.type === 'section'),
    [doc.elements]
  )

  const handleExport = async (opts) => {
    if (exporting || !onExport) return
    setExporting(true)
    try {
      await onExport(opts)
    } finally {
      setExporting(false)
    }
  }

  return (
    <div className="h-full flex flex-col w-full">
      {/* Toolbar - 固定在顶部 */}
      {onExport && (
        <div className="flex items-center justify-between px-4 py-1.5 bg-muted/50 border-b border-border shrink-0">
          <div className="flex items-center gap-2">
            <svg className="w-4 h-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
            </svg>
            <span className="text-xs font-medium text-muted-foreground">{doc.title}</span>
          </div>
          <div className="flex items-center gap-2">
            {sections.length > 0 && (
              <button
                onClick={() => setSidebarOpen((v) => !v)}
                className={`inline-flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium rounded-md border transition-colors ${
                  sidebarOpen
                    ? 'border-primary bg-primary/10 text-primary'
                    : 'border-border bg-background hover:bg-accent hover:text-accent-foreground'
                }`}
              >
                <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
                </svg>
                目录
              </button>
            )}
            <DropdownMenu>
            <DropdownMenuTrigger
              disabled={exporting}
              className="inline-flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium rounded-md border border-border bg-background hover:bg-accent hover:text-accent-foreground transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {exporting ? (
                <>
                  <svg className="w-3.5 h-3.5 animate-spin" fill="none" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                  </svg>
                  导出中...
                </>
              ) : (
                <>
                  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 10v6m0 0l-3-3m3 3l3-3m3 7H9a2 2 0 01-2-2V6a2 2 0 012-2h6a2 2 0 012 2v8a2 2 0 01-2 2h-1.5" />
                  </svg>
                  导出
                  <svg className="w-3 h-3 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                  </svg>
                </>
              )}
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-36">
              <DropdownMenuItem onClick={() => handleExport({ format: 'pdf' })}>
                <svg className="w-3.5 h-3.5 mr-2 text-red-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
                </svg>
                PDF
              </DropdownMenuItem>
              {/*
              <DropdownMenuItem onClick={() => onExport({ format: 'docx' })}>
                <svg className="w-3.5 h-3.5 mr-2 text-blue-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
                DOCX
              </DropdownMenuItem>
              */}
              <DropdownMenuItem onClick={() => handleExport({ format: 'html' })}>
                <svg className="w-3.5 h-3.5 mr-2 text-green-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
                </svg>
                HTML
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          </div>
        </div>
      )}

      {/* Document content - 充满剩余空间，自己滚动 */}
      <div className="flex-1 min-h-0 flex w-full">
        {sidebarOpen && sections.length > 0 && (
          <nav className="w-56 shrink-0 border-r border-border bg-muted/20 flex flex-col">
            <div className="px-3 py-2.5 border-b border-border text-xs font-medium text-muted-foreground uppercase tracking-wider">
              目录
            </div>
            <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
              {sections.map((s) => (
                <button
                  key={s.id}
                  onClick={() => {
                    const el = document.getElementById(`section-${s.id}`)
                    el?.scrollIntoView({ behavior: 'smooth' })
                  }}
                  className="block w-full text-left text-sm py-2 px-3 rounded-md hover:bg-accent truncate text-muted-foreground hover:text-accent-foreground transition-colors"
                  title={s.title || s.id}
                >
                  {s.title || s.id}
                </button>
              ))}
            </div>
          </nav>
        )}
        <div className="flex-1 min-h-0 overflow-auto">
          <div style={{ maxWidth: pageWidth, margin: '0 auto', padding: '1.5rem' }}>
          {/* Document title */}
          <h1 className="text-2xl font-bold mb-4 text-base-content">{doc.title}</h1>

          {/* Render elements */}
          {doc.elements.map((el, idx) => (
            <ElementRenderer key={el.id || idx} element={el} images={images} />
          ))}
          </div>
        </div>
      </div>
    </div>
  )
}

function ElementRenderer({ element, images }) {
  const { type } = element

  switch (type) {
    case 'section':
      return <SectionRenderer element={element} images={images} />
    case 'row':
      return <RowRenderer element={element} images={images} />
    case 'column':
      return <ColumnRenderer element={element} images={images} />
    case 'heading':
      return <HeadingRenderer element={element} />
    case 'paragraph':
      return <ParagraphRenderer element={element} />
    case 'markdown':
      return <MarkdownRenderer element={element} />
    case 'summary_cards':
      return <SummaryCardsRenderer element={element} />
    case 'chart':
      return <ChartRenderer element={element} images={images} />
    case 'table':
      return <TableRenderer element={element} />
    case 'list':
      return <ListRenderer element={element} />
    case 'alert':
      return <AlertRenderer element={element} />
    case 'divider':
      return <DividerRenderer element={element} />
    case 'image':
      return <ImageRenderer element={element} />
    default:
      return <div className="text-xs text-gray-400">Unknown element type: {type}</div>
  }
}

function SectionRenderer({ element, images }) {
  return (
    <section id={`section-${element.id}`} className="section mb-6">
      {element.title && (
        <div className="text-lg font-semibold border-b-2 border-base-300 pb-2 mb-4">
          {element.title}
        </div>
      )}
      {element.elements.map((child, idx) => (
        <ElementRenderer key={child.id || idx} element={child} images={images} />
      ))}
    </section>
  )
}

function RowRenderer({ element, images }) {
  const gap = element.gap ?? 16
  return (
    <div className="flex flex-row flex-wrap" style={{ gap: `${gap}px` }}>
      {element.elements.map((child, idx) => (
        <ElementRenderer key={child.id || idx} element={child} images={images} />
      ))}
    </div>
  )
}

function ColumnRenderer({ element, images }) {
  const flex = element.flex || 1
  const gap = element.gap ?? 8
  return (
    <div className="flex flex-col min-w-0" style={{ flex, gap: `${gap}px` }}>
      {element.elements.map((child, idx) => (
        <ElementRenderer key={child.id || idx} element={child} images={images} />
      ))}
    </div>
  )
}

function HeadingRenderer({ element }) {
  const tag = `h${Math.min(element.level, 4)}`
  const sizes = { 1: 'text-2xl', 2: 'text-xl', 3: 'text-lg', 4: 'text-base' }
  const Tag = tag
  return (
    <Tag className={`font-semibold my-3 ${sizes[element.level] || 'text-base'}`}>
      {element.content}
    </Tag>
  )
}

function ParagraphRenderer({ element }) {
  return <p className="my-2 text-base-content leading-relaxed">{element.content}</p>
}

function MarkdownRenderer({ element }) {
  // Simple markdown: render as pre-formatted text
  // For full markdown support, integrate react-markdown
  return (
    <div className="my-2">
      <pre className="whitespace-pre-wrap text-sm bg-base-200 rounded p-3 font-mono">
        {element.content}
      </pre>
    </div>
  )
}

function SummaryCardsRenderer({ element }) {
  return (
    <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 my-3">
      {element.cards.map((card, idx) => {
        const trendColor = card.trend_type === 'up' ? 'text-green-600' :
                           card.trend_type === 'down' ? 'text-red-600' : ''
        return (
          <div key={idx} className="bg-base-200 border border-base-300 rounded-lg p-4">
            <div className="text-sm text-base-content/60">{card.label}</div>
            <div className="text-xl font-bold mt-1">{card.value}</div>
            {card.trend && (
              <div className={`text-sm mt-1 ${trendColor}`}>{card.trend}</div>
            )}
          </div>
        )
      })}
    </div>
  )
}

function ChartRenderer({ element, images }) {
  const chartKey = `chart-${element.id || ''}`
  const externalImage = images[chartKey] || images[element.id || '']

  // If external image provided, render it
  if (externalImage?.url) {
    return (
      <div className="my-4 text-center">
        {element.title && <div className="text-base font-medium mb-2">{element.title}</div>}
        <img src={externalImage.url} alt={element.title || 'chart'} className="max-w-full h-auto border border-base-300 rounded" />
      </div>
    )
  }

  // Otherwise, render with ECharts
  const option = useMemo(() => buildEChartsOption(element), [element])
  if (!option) {
    return <div className="text-xs text-gray-400">无法渲染图表: {element.title || element.chart_type}</div>
  }

  return (
    <div className="my-4">
      {element.title && <div className="text-base font-medium mb-2">{element.title}</div>}
      <div className="border border-base-300 rounded overflow-hidden">
        <ReactECharts option={option} style={{ height: 400, width: '100%' }} />
      </div>
    </div>
  )
}

function buildEChartsOption(el) {
  const { chart_type, data, config } = el
  if (!data || !data.categories || !data.series) return null

  // Pie/ring charts need completely different config
  if (chart_type === 'pie' || chart_type === 'ring') {
    const series = data.series[0]
    if (!series) return null

    const pieData = data.categories.map((name, i) => ({
      name,
      value: series.values[i] ?? 0,
    }))

    return {
      tooltip: { trigger: 'item' },
      legend: {
        data: data.categories,
        show: config?.show_legend !== false,
        orient: 'vertical',
        left: 0,
        top: 'middle',
      },
      series: [{
        type: 'pie',
        radius: chart_type === 'ring' ? ['35%', '55%'] : ['0%', '55%'],
        center: ['55%', '50%'],
        data: pieData,
        label: {
          show: true,
          formatter: '{b}: {c} ({d}%)',
        },
        emphasis: {
          label: {
            fontSize: 14,
            fontWeight: 'bold',
          },
        },
      }],
    }
  }

  const seriesList = data.series.map((s, i) => ({
    name: s.name,
    type: mapChartType(s.chart_type || chart_type),
    data: s.values,
    yAxisIndex: s.y_axis_index ?? 0,
    smooth: s.smooth ?? ((s.chart_type || chart_type) === 'line' || chart_type === 'area'),
    areaStyle: (s.chart_type || chart_type) === 'area' ? {} : undefined,
  }))

  // Check if mixed chart types (e.g. bar + line) — need dual y-axes
  const seriesTypes = data.series.map((s) => s.chart_type || chart_type)
  const hasMixedAxes = [...new Set(seriesTypes)].length > 1

  const option = {
    tooltip: { trigger: 'axis' },
    legend: { data: data.series.map((s) => s.name), show: config?.show_legend !== false },
    grid: { left: 60, right: hasMixedAxes ? 60 : 30, top: 40, bottom: 40 },
    xAxis: {
      type: 'category',
      data: data.categories,
      name: config?.x_axis_name || '',
    },
    yAxis: hasMixedAxes ? [
      { type: 'value', name: data.series[0]?.y_axis_name || config?.y_axis_name || '' },
      { type: 'value', name: data.series[1]?.y_axis_name || '' },
    ] : {
      type: 'value',
      name: config?.y_axis_name || '',
    },
    series: seriesList,
  }
  return option
}

function mapChartType(chart_type) {
  switch (chart_type) {
    case 'line': return 'line'
    case 'area': return 'line'
    case 'bar':
    case 'column': return 'bar'
    case 'pie':
    case 'ring': return 'pie'
    case 'scatter': return 'scatter'
    case 'radar': return 'radar'
    default: return 'bar'
  }
}

function TableRenderer({ element }) {
  const { columns, rows } = element
  if (!columns || !rows) return null

  const alignClass = (align) => {
    switch (align) {
      case 'right': return 'text-right'
      case 'center': return 'text-center'
      default: return 'text-left'
    }
  }

  return (
    <div className="my-4 overflow-x-auto">
      {element.title && <div className="text-base font-medium mb-2">{element.title}</div>}
      <table className="table table-sm w-full border-collapse">
        <thead>
          <tr>
            {columns.map((col) => (
              <th key={col.key} className={`border border-base-300 px-3 py-2 bg-base-200 font-semibold ${alignClass(col.align)}`}>
                {col.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, idx) => (
            <tr key={idx}>
              {columns.map((col) => (
                <td key={col.key} className={`border border-base-300 px-3 py-2 ${alignClass(col.align)}`}>
                  {row[col.key] || ''}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function ListRenderer({ element }) {
  const Tag = element.list_type === 'ordered' ? 'ol' : 'ul'
  return (
    <Tag className="my-2 pl-6 list-disc">
      {element.items.map((item, idx) => (
        <li key={idx} className="my-1">{item}</li>
      ))}
    </Tag>
  )
}

function AlertRenderer({ element }) {
  const typeColors = {
    error: 'bg-red-50 border-red-400 text-red-800',
    warning: 'bg-yellow-50 border-yellow-400 text-yellow-800',
    info: 'bg-blue-50 border-blue-400 text-blue-800',
    success: 'bg-green-50 border-green-400 text-green-800',
  }
  const cls = typeColors[element.alert_type] || typeColors.info

  return (
    <div className={`rounded-lg border-l-4 p-4 my-2 ${cls}`}>
      {element.title && <div className="font-semibold mb-1">{element.title}</div>}
      <div className="text-sm">{element.content}</div>
      {element.confidence != null && (
        <div className="text-xs text-base-content/50 mt-1">置信度: {Math.round(element.confidence * 100)}%</div>
      )}
    </div>
  )
}

function DividerRenderer() {
  return <hr className="border-base-300 my-4" />
}

function ImageRenderer({ element }) {
  return (
    <div className="my-4 text-center">
      <img
        src={element.src}
        alt={element.alt || ''}
        className="max-w-full h-auto mx-auto"
        style={element.width ? { width: element.width } : {}}
      />
      {element.caption && (
        <div className="text-sm text-base-content/60 italic mt-1">{element.caption}</div>
      )}
    </div>
  )
}

/**
 * AuPageFileLoader - Fetches and renders an .aupage.json file from a URL.
 */
export function AuPageFileLoader({ url, images, className, onExport }) {
  const [doc, setDoc] = useState(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState(null)

  useEffect(() => {
    if (!url) return
    setLoading(true)
    fetch(url)
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`)
        return r.json()
      })
      .then(setDoc)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false))
  }, [url])

  if (loading) return <div className="flex justify-center py-8"><span className="loading loading-spinner loading-lg"></span></div>
  if (error) return <div className="text-red-600 p-4">加载失败: {error}</div>

  return (
    <div className="h-full flex flex-col">
      <AuPageRenderer doc={doc} images={images} onExport={onExport} />
    </div>
  )
}
