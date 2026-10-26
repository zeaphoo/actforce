import { useMemo, useRef, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import FileIcon from './FileIcon'
import ExcelPreview from './ExcelPreview'
import DocxPreview from './DocxPreview'
import CsvPreview from './CsvPreview'
import AuPageRenderer, { AuPageFileLoader } from './AuPageRenderer'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './ui/dropdown-menu'

const getFilename = (file) => file?.original_filename || file?.filename || file?.name || ''
const getExt = (name) => (name.split('.').pop() || '').toLowerCase()
const isImage = (file) => file?.content_type?.startsWith('image/') || ['jpg','jpeg','png','gif','bmp','webp','svg'].includes(getExt(getFilename(file)))
const isPDF = (file) => file?.content_type === 'application/pdf' || getExt(getFilename(file)) === 'pdf'
const isCSV = (file) => file?.content_type === 'text/csv' || getExt(getFilename(file)) === 'csv'
const isExcel = (file) => {
  const ext = getExt(getFilename(file))
  return ['xls','xlsx'].includes(ext) || file?.content_type === 'application/vnd.ms-excel' || file?.content_type === 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'
}
const isDocx = (file) => {
  const ext = getExt(getFilename(file))
  return ['doc','docx'].includes(ext) || file?.content_type === 'application/msword' || file?.content_type === 'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
}
const isText = (file) => {
  const ext = getExt(getFilename(file))
  return ['txt','md','json','xml','html','css','js','py','java','cpp','c','h','go','rs','ts','jsx','tsx'].includes(ext) || 
         file?.content_type?.startsWith('text/')
}
const isAuPage = (file) => {
  const name = getFilename(file)
  return name.endsWith('.aupage.json') || name.endsWith('.aupage')
}

// 检测文本内容是否为 Markdown 格式
const isMarkdownContent = (content, fileName) => {
  if (getExt(fileName) === 'md') return true
  if (getExt(fileName) === 'txt') {
    const mdPatterns = [
      /^#{1,6}\s/m,
      /\*\*[^*]+\*\*/,
      /__[^_]+__/,
      /\*[^*]+\*/,
      /_[^_]+_/,
      /^\s*[-*+]\s/m,
      /^\s*\d+\.\s/m,
      /^\s*>\s/m,
      /`[^`]+`/,
      /^\s*```/m,
      /^\s*---+\s*$/m,
      /\[([^\]]+)\]\([^)]+\)/,
      /^!\[.*\]\(.*\)/m,
    ]
    let matchCount = 0
    for (const pattern of mdPatterns) {
      if (pattern.test(content)) {
        matchCount++
        if (matchCount >= 2) return true
      }
    }
    return false
  }
  return false
}

// Markdown 预览组件（基于 react-markdown + GFM + prose 样式）
const SimpleMarkdownPreview = ({ content, fileName }) => {
  const contentRef = useRef(null)
  const [exporting, setExporting] = useState(false)

  const handleExport = async (format) => {
    if (exporting) return
    setExporting(true)
    try {
      if (!contentRef.current) return

      // 将 markdown 渲染为 HTML 用于导出
      const htmlContent = contentRef.current.innerHTML

      // 从原始文件名派生导出文件名
      const baseName = fileName ? fileName.replace(/\.(md|txt)$/i, '') : 'document'

      const response = await fetch('/api/markdown/export', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ content, format, filename: baseName }),
      })

      if (!response.ok) throw new Error(`HTTP ${response.status}`)
      const blob = await response.blob()
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `${baseName}.${format}`
      document.body.appendChild(a)
      a.click()
      document.body.removeChild(a)
      URL.revokeObjectURL(url)
    } catch (e) {
      alert(`导出失败: ${e.message}`)
    } finally {
      setExporting(false)
    }
  }

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      {/* 工具栏 */}
      <div className="flex items-center justify-between px-4 py-1.5 bg-muted/50 border-b border-border shrink-0">
        <span className="text-xs font-medium text-muted-foreground">Markdown 预览</span>
        <div className="flex items-center gap-1.5">
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
              <DropdownMenuItem onClick={() => handleExport('pdf')}>
                <svg className="w-3.5 h-3.5 mr-2 text-red-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
                </svg>
                PDF
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => handleExport('docx')}>
                <svg className="w-3.5 h-3.5 mr-2 text-blue-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
                DOCX
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
      {/* 内容区域 */}
      <div className="flex-1 min-h-0 overflow-auto p-4">
        <div ref={contentRef} className="prose prose-sm dark:prose-invert max-w-none">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>
            {content}
          </ReactMarkdown>
        </div>
      </div>
    </div>
  )
}

const formatFileSize = (bytes = 0) => {
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(Math.max(bytes,1)) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`
}

function FilePreviewList({
  files = [],
  workspaceId,
  taskId,
  agentType,
  onDelete, // (file) => void
  heightClass = 'max-h-80',
  title = '已上传文件',
  columns = 2, // 每行显示的列数：1, 2, 3
  thumbnailSize = 'normal', // 缩略图大小：'small' | 'normal' | 'large'
}) {
  const [filter, setFilter] = useState('')
  const [preview, setPreview] = useState({ open: false, file: null })
  const [hoverPreview, setHoverPreview] = useState({ visible: false, file: null, x: 0, y: 0 })
  const [textContent, setTextContent] = useState('')
  const [textLoading, setTextLoading] = useState(false)
  const wrapperRef = useRef(null)
  const hideTimer = useRef(null)

  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase()
    if (!q) return files
    return files.filter(f => getFilename(f).toLowerCase().includes(q))
  }, [files, filter])

  const buildFileUrl = (file) => {
    if (!file) return ''
    // 1) 后端返回的可直接访问URL
    if (file.static_url) return file.static_url
    // 2) 通过 API 路径获取文件
    const name = getFilename(file)
    if (!name) return ''
    return `/api/files/${encodeURIComponent(name)}`
  }

  const openPreview = async (file) => {
    setPreview({ open: true, file })
    
    // 如果是文本文件，加载内容
    if (isText(file)) {
      setTextLoading(true)
      try {
        const response = await fetch(buildFileUrl(file))
        if (response.ok) {
          const text = await response.text()
          setTextContent(text)
        }
      } catch (e) {
        console.error('加载文本文件失败:', e)
      } finally {
        setTextLoading(false)
      }
    }
  }
  
  const closePreview = () => {
    setPreview({ open: false, file: null })
    setTextContent('')
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-2 gap-2">
        <h3 className="font-medium">{title}</h3>
        <input
          className="input input-bordered input-xs w-48"
          placeholder="按名称过滤"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </div>
      <div ref={wrapperRef} className={`relative rounded-sm border border-base-300 hover:border-primary transition-colors p-2`}>
        <div className={`${heightClass} overflow-y-auto`}>
          <div className={`grid gap-3 ${
            columns === 1 ? 'grid-cols-1' :
            columns === 3 ? 'grid-cols-1 sm:grid-cols-2 lg:grid-cols-3' :
            'grid-cols-1 sm:grid-cols-2' // 默认 2 列
          }`}>
          {filtered.map((file, idx) => (
            <div key={file.id || idx} className="flex items-center justify-between bg-base-200 p-3 rounded gap-2 min-w-0">
              <div className="flex items-center min-w-0 flex-1">
                {isImage(file) ? (
                  <div
                    className={`flex items-center justify-center mr-2 overflow-hidden shrink-0 ${
                      thumbnailSize === 'large' ? 'w-16 h-16' :
                      thumbnailSize === 'small' ? 'w-8 h-8' :
                      'w-10 h-10'
                    }`}
                    onMouseEnter={(e) => {
                      if (!wrapperRef.current) return
                      if (hideTimer.current) clearTimeout(hideTimer.current)
                      const iconRect = e.currentTarget.getBoundingClientRect()
                      const wrapRect = wrapperRef.current.getBoundingClientRect()
                      setHoverPreview({
                        visible: true,
                        file,
                        x: iconRect.left - wrapRect.left + iconRect.width + 8,
                        y: iconRect.top - wrapRect.top + iconRect.height / 2,
                      })
                    }}
                    onMouseLeave={() => {
                      hideTimer.current = setTimeout(() => setHoverPreview((p) => ({ ...p, visible: false })), 80)
                    }}
                  >
                    <img src={buildFileUrl(file)} alt={getFilename(file)} className="object-cover w-full h-full" />
                  </div>
                ) : (
                  <div
                    className={`flex items-center justify-center mr-2 shrink-0 ${
                      thumbnailSize === 'large' ? 'w-16 h-16' :
                      thumbnailSize === 'small' ? 'w-8 h-8' :
                      'w-10 h-10'
                    }`}
                    onMouseEnter={(e) => {
                      if (!wrapperRef.current) return
                      if (hideTimer.current) clearTimeout(hideTimer.current)
                      const iconRect = e.currentTarget.getBoundingClientRect()
                      const wrapRect = wrapperRef.current.getBoundingClientRect()
                      setHoverPreview({
                        visible: true,
                        file,
                        x: iconRect.left - wrapRect.left + iconRect.width + 8,
                        y: iconRect.top - wrapRect.top + iconRect.height / 2,
                      })
                    }}
                    onMouseLeave={() => {
                      hideTimer.current = setTimeout(() => setHoverPreview((p) => ({ ...p, visible: false })), 80)
                    }}
                  >
                    <FileIcon filename={getFilename(file)} className="w-10 h-10 object-contain" />
                  </div>
                )}
                <div className="min-w-0 flex-1">
                  <div className="text-sm font-medium truncate" title={getFilename(file)}>{getFilename(file) || '未知文件'}</div>
                  <div className="text-xs text-gray-500">{formatFileSize(file.file_size || file.size || 0)}</div>
                </div>
              </div>
              <div className="flex items-center gap-1 shrink-0">
                <button className="btn btn-xs" onClick={() => openPreview(file)}>预览</button>
                <a className="btn btn-xs btn-outline" href={buildFileUrl(file)} download>下载</a>
                {onDelete && (
                  <button className="btn btn-xs btn-ghost text-error" onClick={() => onDelete(file)}>删除</button>
                )}
              </div>
            </div>
          ))}
          </div>
        </div>
        {hoverPreview.visible && hoverPreview.file && (
          <div
            className="absolute z-[60] -translate-y-1/2 w-72 bg-base-100 border border-base-300 rounded shadow-lg p-2"
            style={{ left: hoverPreview.x, top: hoverPreview.y }}
            onMouseEnter={() => { if (hideTimer.current) clearTimeout(hideTimer.current) }}
            onMouseLeave={() => { hideTimer.current = setTimeout(() => setHoverPreview((p) => ({ ...p, visible: false })), 80) }}
          >
            {isImage(hoverPreview.file) && (
              <img src={buildFileUrl(hoverPreview.file)} alt={getFilename(hoverPreview.file)} className="max-h-60 w-full object-contain" />
            )}
            {isPDF(hoverPreview.file) && (
              <iframe 
                src={`${buildFileUrl(hoverPreview.file)}#toolbar=0&navpanes=0`}
                className="w-full h-56"
                title="PDF 预览"
              />
            )}
            {isCSV(hoverPreview.file) && (
              <iframe src={buildFileUrl(hoverPreview.file)} title="csv-preview-pop" className="w-full h-48 bg-base-200" />
            )}
            {!isImage(hoverPreview.file) && !isPDF(hoverPreview.file) && !isCSV(hoverPreview.file) && !isExcel(hoverPreview.file) && (
              <div className="text-xs text-base-content/70">
                暂不支持该文件类型的在线预览。
              </div>
            )}
            {isExcel(hoverPreview.file) && (
              <div className="text-xs text-base-content/70">
                Excel 文件请点击"预览"按钮查看
              </div>
            )}
          </div>
        )}
      </div>

      {preview.open && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4" onClick={closePreview}>
          <div className={`bg-base-100 border border-base-300 rounded-xl w-full h-[95vh] flex flex-col shadow-2xl ${isExcel(preview.file) ? 'max-w-[95vw] lg:max-w-[90vw]' : 'max-w-6xl'}`} onClick={(e) => e.stopPropagation()}>
            <div className="flex items-center justify-between px-5 py-3 border-b border-base-300 flex-shrink-0">
              <h3 className="text-base font-semibold truncate">{getFilename(preview.file)}</h3>
              <button className="btn btn-ghost btn-sm btn-circle flex-shrink-0" onClick={closePreview}>✕</button>
            </div>
            <div className="flex-1 min-h-0 px-3 pb-3 flex flex-col">
              {isImage(preview.file) && (
                <div className="flex-1 min-h-0 flex items-center justify-center overflow-auto">
                  <img src={buildFileUrl(preview.file)} alt={getFilename(preview.file)} className="max-w-full max-h-full object-contain" />
                </div>
              )}
              {isPDF(preview.file) && (
                <div className="flex-1 min-h-0 border border-base-300 rounded overflow-hidden">
                  <iframe 
                    src={buildFileUrl(preview.file)}
                    className="w-full h-full"
                    title="PDF 预览"
                  />
                </div>
              )}
              {isCSV(preview.file) && (
                <CsvPreview src={buildFileUrl(preview.file)} filename={getFilename(preview.file)} className="flex-1 min-h-0" />
              )}
              {isExcel(preview.file) && (
                <ExcelPreview src={buildFileUrl(preview.file)} filename={getFilename(preview.file)} className="flex-1 min-h-0" />
              )}
              {isDocx(preview.file) && (
                <DocxPreview src={buildFileUrl(preview.file)} filename={getFilename(preview.file)} className="flex-1 min-h-0" />
              )}
              {isAuPage(preview.file) && (
                <div className="h-full">
                  <AuPageFileLoader
                    url={buildFileUrl(preview.file)}
                    onExport={async ({ format }) => {
                      try {
                        const response = await fetch('/api/aupage/export', {
                          method: 'POST',
                          headers: { 'Content-Type': 'application/json' },
                          body: JSON.stringify({
                            input_file: getFilename(preview.file),
                            format,
                          }),
                        })
                        if (!response.ok) throw new Error(`HTTP ${response.status}`)
                        const blob = await response.blob()
                        const a = document.createElement('a')
                        a.href = URL.createObjectURL(blob)
                        a.download = `${getFilename(preview.file).replace(/\.(aupage\.json|aupage)$/, '')}.${format}`
                        a.click()
                      } catch (e) {
                        alert(`导出失败: ${e.message}`)
                      }
                    }}
                  />
                </div>
              )}
              {isText(preview.file) && (
                <div className="flex-1 min-h-0 flex flex-col">
                  {textLoading ? (
                    <div className="flex justify-center py-8">
                      <span className="loading loading-spinner loading-lg"></span>
                    </div>
                  ) : isMarkdownContent(textContent, getFilename(preview.file)) ? (
                    <SimpleMarkdownPreview content={textContent} fileName={getFilename(preview.file)} />
                  ) : (
                    <pre className="flex-1 min-h-0 whitespace-pre-wrap break-words text-sm bg-base-200 rounded p-4 overflow-auto font-mono">
                      {textContent || '无法加载文件内容'}
                    </pre>
                  )}
                </div>
              )}
              {!isImage(preview.file) && !isPDF(preview.file) && !isCSV(preview.file) && !isExcel(preview.file) && !isDocx(preview.file) && !isText(preview.file) && !isAuPage(preview.file) && (
                <div className="text-center py-16">
                  <p className="text-sm text-base-content/70">暂不支持该文件类型的在线预览。您可以下载后查看。</p>
                  <a className="btn btn-sm btn-outline mt-4" href={buildFileUrl(preview.file)} download>下载文件</a>
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

export default FilePreviewList
