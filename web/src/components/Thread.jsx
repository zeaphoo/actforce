import {
  ComposerPrimitive,
  ThreadPrimitive,
  MessagePrimitive,
  MessagePartPrimitive,
  ActionBarPrimitive,
  useAuiState,
  MarkdownTextPrimitive,
  StoreContext,
  useExternalStoreRuntime,
  AssistantRuntimeProvider,
} from '@actforce/chat-ui'
import { Send, Bot, User, ArrowUp, Copy, Check, Paperclip, X, Image, FileText, Sparkles, Code, FileQuestion, Lightbulb, Download, Eye, Loader2 } from 'lucide-react'
import { useState, useRef, useEffect, useCallback, useMemo, createContext, useContext, Component } from 'react'

// Error boundary to catch useAuiState errors gracefully
class MessageErrorBoundary extends Component {
  constructor(props) {
    super(props)
    this.state = { hasError: false }
  }
  static getDerivedStateFromError() {
    return { hasError: true }
  }
  render() {
    if (this.state.hasError) return null
    return this.props.children
  }
}
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { useShallow } from 'zustand/shallow'
import { useThreadStore, useThreadIsRunning } from '../lib/threadStore'
import { useInferenceHealthContext } from '../App'
import { getAuthHeaders } from '../lib/auth'
import { useAuth } from '../lib/AuthContext'
import { useAppConfig } from '../lib/AppConfigContext'
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
import { StepStatusContainer } from './StepStatus'
import { toast } from './Toast'

/** 将秒数格式化为人类可读的耗时字符串 */
function formatDuration(seconds) {
  if (seconds < 60) return `${seconds}s`
  if (seconds < 3600) {
    const m = Math.floor(seconds / 60)
    const s = seconds % 60
    return `${m}m ${s}s`
  }
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  const s = seconds % 60
  return `${h}h ${m}m ${s}s`
}

/** 将 ISO 时间戳格式化为带日期的本地时间字符串 */
function formatCompletionTime(iso) {
  if (!iso) return ''
  const d = new Date(iso)
  const y = d.getFullYear()
  const mo = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  const h = String(d.getHours()).padStart(2, '0')
  const mi = String(d.getMinutes()).padStart(2, '0')
  const s = String(d.getSeconds()).padStart(2, '0')
  return `${y}-${mo}-${day} ${h}:${mi}:${s}`
}

// 排队状态显示组件
const QueueStatus = ({ position }) => (
  <div className="flex items-center gap-3 py-3 px-4 mx-auto w-full max-w-[--thread-max-width] rounded-xl bg-amber-50 border border-amber-200 shadow-sm">
    <div className="flex items-center gap-1 shrink-0">
      <div className="w-2.5 h-2.5 rounded-full bg-amber-500 animate-pulse" style={{ animationDelay: '0ms' }} />
      <div className="w-2.5 h-2.5 rounded-full bg-amber-500 animate-pulse" style={{ animationDelay: '200ms' }} />
      <div className="w-2.5 h-2.5 rounded-full bg-amber-500 animate-pulse" style={{ animationDelay: '400ms' }} />
    </div>
    <div className="flex flex-col min-w-0">
      <span className="text-sm font-medium text-amber-800">正在排队等待</span>
      <span className="text-xs text-amber-600">前面还有约 {position} 个会话在处理中</span>
    </div>
  </div>
)

// 创建填入上下文
const FillToInputContext = createContext(null)
const StepStatusContext = createContext(null)
const StreamingMessageIdContext = createContext(null)
const ThreadIdContext = createContext(null)

export const useFillToInput = () => useContext(FillToInputContext)
export const useStepStatus = () => useContext(StepStatusContext)
export const useStreamingMessageId = () => useContext(StreamingMessageIdContext)
export const useThreadId = () => useContext(ThreadIdContext)

// Helper to extract message metadata and attachments
const getMessageAttachments = (message) => {
  const metadata = message?.metadata?.custom || message?.metadata || {}
  return metadata?.attachments || []
}

// 附件显示组件
const MessageAttachments = ({ attachments }) => {
  if (!attachments || attachments.length === 0) return null

  const [previewModal, setPreviewModal] = useState({ open: false, file: null, content: null })
  const [downloadingName, setDownloadingName] = useState(null)

  const formatFileSize = (bytes) => {
    if (!bytes) return ''
    if (bytes < 1024) return bytes + ' B'
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB'
    return (bytes / (1024 * 1024)).toFixed(1) + ' MB'
  }

  // 获取文件扩展名
  const getExt = (name) => (name?.split('.').pop() || '').toLowerCase()

  // 根据扩展名返回文件图标路径（public/file-icons/ 目录）
  const getFileIconPath = (type, name) => {
    // 优先按完整文件名匹配（处理复合扩展名如 .aupage.json）
    if (name?.endsWith('.aupage.json') || name?.endsWith('.aupage')) return '/file-icons/aupage.png'
    const ext = getExt(name)
    const iconMap = {
      bmp: 'bmp', csv: 'csv', doc: 'doc', docx: 'docx', epub: 'epub',
      flac: 'flac', gif: 'gif', hex: 'hex', htm: 'htm', html: 'html',
      jpg: 'jpg', jpeg: 'jpg', md: 'md', mov: 'mov', mp3: 'mp3',
      mp4: 'mp4', pdf: 'pdf', png: 'png', ppt: 'ppt', pptx: 'pptx',
      psd: 'psd', rar: 'rar', svg: 'svg', txt: 'txt', wav: 'wav',
      xls: 'xlsx', xlsx: 'xlsx', xml: 'xml', zip: 'zip',
    }
    return '/file-icons/' + (iconMap[ext] || 'default') + '.png'
  }

  // 判断文件类型
  const isImage = (file) => file.type?.startsWith('image/')
  const isPDF = (file) => getExt(file.name) === 'pdf' || file.type === 'application/pdf'
  const isExcel = (file) => ['xlsx', 'xls'].includes(getExt(file.name))
  const isDocx = (file) => ['docx', 'doc'].includes(getExt(file.name))
  const isCSV = (file) => getExt(file.name) === 'csv' || file.type === 'text/csv'
  const isText = (file) => ['txt', 'md', 'json', 'xml', 'html', 'css', 'js', 'py', 'java', 'cpp', 'c', 'h', 'go', 'rs', 'ts', 'jsx', 'tsx'].includes(getExt(file.name))
  const isAuPage = (file) => file.name.endsWith('.aupage.json') || file.name.endsWith('.aupage')

  // 检测文本内容是否为 Markdown 格式
  const isMarkdownContent = (content, fileName) => {
    // .md 文件直接认为是 markdown
    if (getExt(fileName) === 'md') return true
    // .txt 文件：检测 markdown 语法特征
    if (getExt(fileName) === 'txt') {
      const mdPatterns = [
        /^#{1,6}\s/m,           // headers: # ## ###
        /\*\*[^*]+\*\*/,        // bold: **text**
        /__[^_]+__/,            // bold: __text__
        /\*[^*]+\*/,            // italic: *text*
        /_[^_]+_/,              // italic: _text_
        /^\s*[-*+]\s/m,         // unordered list: - item
        /^\s*\d+\.\s/m,         // ordered list: 1. item
        /^\s*>\s/m,             // blockquote: > text
        /`[^`]+`/,              // inline code: `code`
        /^\s*```/m,             // code block
        /^\s*---+\s*$/m,        // horizontal rule
        /\[([^\]]+)\]\([^)]+\)/, // link: [text](url)
        /^!\[.*\]\(.*\)/m,      // image: ![alt](url)
      ]
      // 匹配到 2 个以上 markdown 特征才认为是 markdown
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
    const [exporting, setExporting] = useState(false)

    const handleExport = async (format) => {
      if (exporting) return
      setExporting(true)
      try {
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
          <div className="prose prose-sm dark:prose-invert max-w-none">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>
              {content}
            </ReactMarkdown>
          </div>
        </div>
      </div>
    )
  }

  const handleDownload = async (file) => {
    if (!file.url || downloadingName) return

    const token = localStorage.getItem('token')
    const authHeaders = token ? { Authorization: `Bearer ${token}` } : {}
    const needsConversion = file.name?.endsWith('.aupage.json') || file.name?.endsWith('.aupage') || getExt(file.name) === 'md'

    const downloadBlob = (blob, filename) => {
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = filename
      document.body.appendChild(a)
      a.click()
      document.body.removeChild(a)
      URL.revokeObjectURL(url)
    }

    setDownloadingName(file.name)
    let processingToast = null
    if (needsConversion) {
      processingToast = toast.info('正在转换为 PDF，请稍候...', 0)
    }

    try {
      // .aupage / .aupage.json → 调用后端转 PDF 下载
      if (file.name?.endsWith('.aupage.json') || file.name?.endsWith('.aupage')) {
        const resp = await fetch('/api/aupage/export', {
          method: 'POST',
          headers: { ...authHeaders, 'Content-Type': 'application/json' },
          body: JSON.stringify({ input_file: file.name, format: 'pdf' }),
        })
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`)
        const blob = await resp.blob()
        const pdfName = file.name.replace(/\.(aupage(\.json)?)$/, '.pdf')
        downloadBlob(blob, pdfName)
        return
      }

      // .md → 先获取内容，再调用后端转 PDF 下载
      if (getExt(file.name) === 'md') {
        const contentResp = await fetch(file.url, { headers: authHeaders })
        if (!contentResp.ok) throw new Error(`HTTP ${contentResp.status}`)
        const content = await contentResp.text()

        const resp = await fetch('/api/markdown/export', {
          method: 'POST',
          headers: { ...authHeaders, 'Content-Type': 'application/json' },
          body: JSON.stringify({ content, format: 'pdf', filename: file.name.replace(/\.md$/, '') }),
        })
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`)
        const blob = await resp.blob()
        const pdfName = file.name.replace(/\.md$/, '.pdf')
        downloadBlob(blob, pdfName)
        return
      }

      // 其他文件 → 直接下载
      const a = document.createElement('a')
      a.href = file.url
      a.download = file.name || 'download'
      a.target = '_blank'
      document.body.appendChild(a)
      a.click()
      document.body.removeChild(a)
    } catch (e) {
      console.error('下载失败:', e)
      toast.error('PDF 转换失败，请重试')
    } finally {
      if (processingToast) toast.dismiss(processingToast)
      setDownloadingName(null)
    }
  }

  const handlePreview = async (file) => {
    setPreviewModal({ open: true, file, content: null, blobUrl: null })

    if (!file.url) return

    const token = localStorage.getItem('token')
    const headers = token ? { Authorization: `Bearer ${token}` } : {}

    // PDF：fetch 获取 blob URL，避免 iframe 无法携带 Authorization 的问题
    if (isPDF(file)) {
      try {
        const response = await fetch(file.url, { headers })
        if (response.ok) {
          const blob = await response.blob()
          const blobUrl = URL.createObjectURL(blob)
          setPreviewModal(prev => ({ ...prev, blobUrl }))
        }
      } catch (e) {
        console.error('加载 PDF 失败:', e)
      }
      return
    }

    // AuPage 文件：不需要预加载，由渲染器处理
    if (isAuPage(file)) {
      return
    }

    // 文本文件：fetch 获取文本内容
    if (isText(file)) {
      try {
        const response = await fetch(file.url, { headers })
        if (response.ok) {
          const text = await response.text()
          setPreviewModal(prev => ({ ...prev, content: text }))
        }
      } catch (e) {
        console.error('加载文本文件失败:', e)
      }
    }
  }

  const closePreview = () => {
    // 释放 blob URL
    setPreviewModal(prev => {
      if (prev.blobUrl) URL.revokeObjectURL(prev.blobUrl)
      return { open: false, file: null, content: null, blobUrl: null }
    })
  }

  return (
    <>
      <div className="flex flex-wrap gap-1.5 mt-1.5">
        {attachments.map((file, idx) => {
          const iconPath = getFileIconPath(file.type, file.name)

          return (
            <div
              key={idx}
              className="flex items-center gap-2 px-2.5 py-1.5 rounded-lg border border-border/50 bg-background/60 backdrop-blur-sm hover:bg-muted/50 hover:border-border transition-all group shadow-sm min-w-[140px] max-w-[220px]"
            >
              <div className="w-7 h-7 rounded-md flex items-center justify-center flex-shrink-0 overflow-hidden">
                <img src={iconPath} alt={getExt(file.name)} className="w-5 h-5 object-contain" />
              </div>
              <div className="flex-1 min-w-0">
                <div className="text-xs font-medium text-foreground truncate leading-tight" title={file.name}>
                  {file.name}
                </div>
                <div className="text-[10px] text-muted-foreground mt-0.5">
                  {formatFileSize(file.size)}
                </div>
              </div>
              <div className="flex items-center gap-1 flex-shrink-0">
                {file.url && (
                  <button
                    onClick={() => handlePreview(file)}
                    className="p-1 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground"
                    title="预览"
                  >
                    <Eye className="w-3 h-3" />
                  </button>
                )}
                {file.url && (
                  <button
                    onClick={() => handleDownload(file)}
                    disabled={downloadingName === file.name}
                    className="p-1 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed"
                    title={downloadingName === file.name ? '正在转换...' : '下载'}
                  >
                    {downloadingName === file.name ? (
                      <Loader2 className="w-3 h-3 animate-spin" />
                    ) : (
                      <Download className="w-3 h-3" />
                    )}
                  </button>
                )}
              </div>
            </div>
          )
        })}
      </div>

      {/* 预览模态框 */}
      {previewModal.open && previewModal.file && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4" onClick={closePreview}>
          <div className={`bg-background border border-border rounded-xl w-full h-[95vh] flex flex-col shadow-2xl ${isExcel(previewModal.file) ? 'max-w-[95vw] lg:max-w-[90vw]' : 'max-w-7xl'}`} onClick={(e) => e.stopPropagation()}>
            {/* 头部 */}
            <div className="flex items-center justify-between px-5 py-3 border-b border-border flex-shrink-0">
              <h3 className="text-base font-semibold truncate">{previewModal.file.name}</h3>
              <button
                onClick={closePreview}
                className="btn btn-ghost btn-sm btn-circle flex-shrink-0"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            {/* 内容区域 - 预览组件充满空间 */}
            <div className="flex-1 min-h-0 flex flex-col px-3 pb-3">
              {/* 图片预览 */}
              {isImage(previewModal.file) && (
                <div className="flex-1 min-h-0 flex items-center justify-center overflow-auto">
                  <img
                    src={previewModal.file.url}
                    alt={previewModal.file.name}
                    className="max-w-full max-h-full object-contain"
                  />
                </div>
              )}

              {/* PDF预览 */}
              {isPDF(previewModal.file) && (
                <div className="flex-1 min-h-0 border border-border rounded overflow-hidden flex items-center justify-center">
                  {previewModal.blobUrl ? (
                    <iframe
                      src={previewModal.blobUrl}
                      className="w-full h-full"
                      title="PDF 预览"
                    />
                  ) : (
                    <span className="loading loading-spinner loading-lg"></span>
                  )}
                </div>
              )}

              {/* CSV预览 */}
              {isCSV(previewModal.file) && (
                <CsvPreview
                  src={previewModal.file.url}
                  filename={previewModal.file.name}
                  className="flex-1 min-h-0"
                />
              )}

              {/* Excel预览 */}
              {isExcel(previewModal.file) && (
                <ExcelPreview
                  src={previewModal.file.url}
                  filename={previewModal.file.name}
                  className="flex-1 min-h-0"
                />
              )}

              {/* Word 文档预览 */}
              {isDocx(previewModal.file) && (
                <DocxPreview
                  src={previewModal.file.url}
                  filename={previewModal.file.name}
                  className="flex-1 min-h-0"
                />
              )}

              {/* AuPage 文档预览 */}
              {isAuPage(previewModal.file) && (
                <div className="h-full">
                  <AuPageFileLoader
                    url={previewModal.file.url}
                    onExport={async ({ format }) => {
                      try {
                        const response = await fetch('/api/aupage/export', {
                          method: 'POST',
                          headers: { 'Content-Type': 'application/json' },
                          body: JSON.stringify({
                            input_file: previewModal.file.name,
                            format,
                          }),
                        })
                        if (!response.ok) throw new Error(`HTTP ${response.status}`)
                        const blob = await response.blob()
                        const a = document.createElement('a')
                        a.href = URL.createObjectURL(blob)
                        a.download = `${previewModal.file.name.replace(/\.(aupage\.json|aupage)$/, '')}.${format}`
                        a.click()
                      } catch (e) {
                        alert(`导出失败: ${e.message}`)
                      }
                    }}
                  />
                </div>
              )}

              {/* 文本文件预览 */}
              {isText(previewModal.file) && (
                <div className="flex-1 min-h-0 flex flex-col">
                  {previewModal.content === null ? (
                    <div className="flex justify-center py-8">
                      <span className="loading loading-spinner loading-lg"></span>
                    </div>
                  ) : isMarkdownContent(previewModal.content, previewModal.file.name) ? (
                    <SimpleMarkdownPreview content={previewModal.content} fileName={previewModal.file.name} />
                  ) : (
                    <pre className="flex-1 min-h-0 whitespace-pre-wrap break-words text-sm bg-muted rounded p-4 overflow-auto font-mono border border-border">
                      {previewModal.content || '无法加载文件内容'}
                    </pre>
                  )}
                </div>
              )}

              {/* 不支持的类型 */}
              {!isImage(previewModal.file) && !isPDF(previewModal.file) && !isCSV(previewModal.file) && !isExcel(previewModal.file) && !isDocx(previewModal.file) && !isText(previewModal.file) && !isAuPage(previewModal.file) && (
                <div className="text-center py-16">
                  <div className="text-muted-foreground mb-4">
                    <p>暂不支持该文件类型的在线预览。</p>
                    <p className="text-sm mt-2">您可以下载后查看。</p>
                  </div>
                  <button
                    onClick={() => handleDownload(previewModal.file)}
                    className="btn btn-outline btn-sm"
                  >
                    <Download className="w-4 h-4 mr-2" />
                    下载文件
                  </button>
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </>
  )
}

// 用户消息(带自动填入按钮)
const UserMessageWithFill = ({ onFillToInput, message }) => {
  const isCopied = useAuiState((s) => s.message.isCopied)
  const messageContent = useAuiState((s) => s.message.content)

  // assistant-ui 将 metadata 包装在 custom 属性中
  const attachments = getMessageAttachments(message)
  const textContent = messageContent?.find(part => part.type === 'text')?.text || ''

  return (
    <MessagePrimitive.Root className="group flex items-start gap-3 justify-end py-4 mx-auto w-full max-w-[--thread-max-width]">
      <div className="max-w-[80%] space-y-2">
        {/* 消息文本气泡 */}
        <div className="bg-primary text-primary-foreground px-4 py-2 rounded-2xl rounded-tr-sm">
          {textContent && <MessagePrimitive.Content />}
        </div>

        {/* 附件区域 - 独立于文本气泡 */}
        {attachments.length > 0 && (
          <div className="flex justify-end">
            <MessageAttachments attachments={attachments} />
          </div>
        )}

        {/* 用户消息操作按钮 */}
        <div className="flex items-center gap-1 justify-end opacity-0 group-hover:opacity-100 transition-opacity">
          <ActionBarPrimitive.Root className="flex items-center gap-1">
            {/* 复制按钮 */}
            <ActionBarPrimitive.Copy className="h-7 w-7 rounded-md hover:bg-muted transition-colors flex items-center justify-center text-muted-foreground hover:text-foreground">
              {isCopied ? (
                <Check className="h-3.5 w-3.5 text-green-500" />
              ) : (
                <Copy className="h-3.5 w-3.5" />
              )}
            </ActionBarPrimitive.Copy>

            {/* 自动填入按钮 */}
            {onFillToInput && (
              <button
                onClick={() => onFillToInput(textContent)}
                className="h-7 w-7 rounded-md hover:bg-muted transition-colors flex items-center justify-center text-muted-foreground hover:text-foreground"
                title="填入到输入框"
              >
                <svg className="h-3.5 w-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 10h10a8 8 0 018 8v2M3 10l6 6m-6-6l6-6" />
                </svg>
              </button>
            )}
          </ActionBarPrimitive.Root>
        </div>
      </div>
      <div className="w-8 h-8 rounded-full bg-secondary flex items-center justify-center flex-shrink-0">
        <User className="w-4 h-4 text-secondary-foreground" />
      </div>
    </MessagePrimitive.Root>
  )
}

// 消息组件 - 根据角色显示不同的消息组件
const MessageWithEditComposer = () => {
  const store = useContext(StoreContext)
  if (!store) return null

  const role = useAuiState((s) => s.message?.role)
  const fillToInput = useFillToInput()

  // 获取完整的消息对象
  const message = useAuiState((s) => s.message)

  // 根据角色显示不同的消息组件
  if (role === 'user') {
    return <UserMessageWithFill onFillToInput={fillToInput} message={message} />
  }
  return <AssistantMessage />
}

// Wrapped version with error boundary for extra safety
const SafeMessageWithEditComposer = () => (
  <MessageErrorBoundary>
    <MessageWithEditComposer />
  </MessageErrorBoundary>
)

// 助手消息
const AssistantMessage = () => {
  const isRunning = useAuiState((s) => s.message.status?.type === 'running')
  const isCopied = useAuiState((s) => s.message.isCopied)
  const stepStatuses = useStepStatus()
  const streamingMessageId = useStreamingMessageId()
  const currentMessageId = useAuiState((s) => s.message.id)
  const message = useAuiState((s) => s.message)
  const messageContent = useAuiState((s) => s.message.content)

  // 获取附件
  const attachments = getMessageAttachments(message)

  const shouldShowStepStatuses = streamingMessageId && currentMessageId === streamingMessageId

  return (
    <MessagePrimitive.Root className="group flex items-start gap-3 py-4 mx-auto w-full max-w-[--thread-max-width]">
      <div className="w-8 h-8 rounded-full bg-primary flex items-center justify-center flex-shrink-0">
        <Bot className="w-4 h-4 text-primary-foreground" />
      </div>
      <div className="max-w-[80%] space-y-2">
        {/* 步骤状态显示 + 实时耗时 - 仅在正在流式传输的消息中显示 */}
        {shouldShowStepStatuses && (
          <div className="flex items-center gap-3 mb-3 flex-wrap">
            {stepStatuses && Object.keys(stepStatuses).length > 0 && (
              <div className="max-w-md">
                <StepStatusContainer statuses={stepStatuses} />
              </div>
            )}
          </div>
        )}

        <div className="prose prose-sm dark:prose-invert max-w-none">
          <MessagePrimitive.Content components={{ Text: MarkdownTextPrimitive }} />
          {/* 等待动画 - 当消息正在流式传输时显示 */}
          {isRunning && (
            <div className="flex items-center gap-1 py-2">
              <span
                className="w-2 h-2 rounded-full bg-muted-foreground/50 animate-pulse"
                style={{ animationDelay: '0ms' }}
              />
              <span
                className="w-2 h-2 rounded-full bg-muted-foreground/50 animate-pulse"
                style={{ animationDelay: '200ms' }}
              />
              <span
                className="w-2 h-2 rounded-full bg-muted-foreground/50 animate-pulse"
                style={{ animationDelay: '400ms' }}
              />
            </div>
          )}
        </div>

        {/* 附件区域 */}
        {attachments.length > 0 && (
          <MessageAttachments attachments={attachments} />
        )}

        {/* 历史消息耗时与完成时间 */}
        {!isRunning && message.metadata?.custom?.elapsed_display && (
          <div className="flex items-center gap-2 text-xs text-muted-foreground mt-1">
            <span>耗时 {message.metadata.custom.elapsed_display}</span>
            {message.metadata.custom.completed_at && (
              <span>· 完成于 {formatCompletionTime(message.metadata.custom.completed_at)}</span>
            )}
          </div>
        )}

        {/* 消息操作按钮 - 仅在消息完成后显示 */}
        {!isRunning && (
          <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
            <ActionBarPrimitive.Root className="flex items-center gap-1">
              {/* 复制按钮 */}
              <ActionBarPrimitive.Copy className="h-7 w-7 rounded-md hover:bg-muted transition-colors flex items-center justify-center text-muted-foreground hover:text-foreground">
                {isCopied ? (
                  <Check className="h-3.5 w-3.5 text-green-500" />
                ) : (
                  <Copy className="h-3.5 w-3.5" />
                )}
              </ActionBarPrimitive.Copy>
            </ActionBarPrimitive.Root>
          </div>
        )}
      </div>
    </MessagePrimitive.Root>
  )
}

// 功能提示卡片组件
const SuggestionCard = ({ icon, title, description, onClick }) => {
  return (
    <button
      onClick={onClick}
      className="flex items-start gap-3 p-4 rounded-xl border border-border bg-card hover:bg-muted/50 hover:border-primary/30 transition-all text-left group"
    >
      <div className="w-10 h-10 rounded-lg bg-primary/10 flex items-center justify-center text-primary flex-shrink-0 group-hover:bg-primary/20 transition-colors">
        {icon}
      </div>
      <div className="flex-1 min-w-0">
        <h3 className="font-medium text-foreground text-sm">{title}</h3>
        <p className="text-xs text-muted-foreground mt-0.5 line-clamp-2">{description}</p>
      </div>
    </button>
  )
}

// 滚动到底部按钮
const ThreadScrollToBottom = () => {
  return (
    <ThreadPrimitive.ScrollToBottom
      className="absolute -top-12 left-1/2 -translate-x-1/2 rounded-full shadow-md h-9 w-9 bg-white border-2 border-border hover:bg-gray-50 transition-all flex items-center justify-center"
    >
      <svg className="w-4 h-4 text-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 14l-7 7m0 0l-7-7m7 7V3" />
      </svg>
    </ThreadPrimitive.ScrollToBottom>
  )
}

// 附件预览组件（Composer中的附件）
const AttachmentPreview = ({ attachment, onRemove, onPreview }) => {
  const isImage = attachment.file?.type?.startsWith('image/')

  return (
    <div className="relative group">
      {isImage && attachment.previewUrl ? (
        <div
          className="relative w-16 h-16 rounded-lg overflow-hidden border border-border cursor-pointer hover:border-primary transition-colors"
          onClick={() => onPreview?.(attachment)}
        >
          <img
            src={attachment.previewUrl}
            alt={attachment.file.name}
            className="w-full h-full object-cover"
          />
          <div className="absolute inset-0 bg-black/0 group-hover:bg-black/20 transition-colors flex items-center justify-center">
            <Eye className="w-5 h-5 text-white opacity-0 group-hover:opacity-100 transition-opacity" />
          </div>
        </div>
      ) : (
        <div
          className="flex items-center gap-2 px-3 py-2 rounded-lg bg-muted border border-border cursor-pointer hover:border-primary transition-colors"
          onClick={() => onPreview?.(attachment)}
        >
          <FileText className="w-4 h-4 text-muted-foreground" />
          <span className="text-sm text-foreground max-w-32 truncate" title={attachment.file.name}>
            {attachment.file.name}
          </span>
          <Eye className="w-3 h-3 text-muted-foreground ml-1" />
        </div>
      )}
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation()
          onRemove()
        }}
        className="absolute -top-2 -right-2 w-5 h-5 rounded-full bg-destructive text-destructive-foreground flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity shadow-sm"
      >
        <X className="w-3 h-3" />
      </button>
    </div>
  )
}

// 输入框组件
const Composer = ({ onAttachmentsChange, clearAttachmentsRef, fillToInputRef, disabled }) => {
  const [attachments, setAttachments] = useState([])
  const [previewModal, setPreviewModal] = useState({ open: false, attachment: null, content: null })
  const fileInputRef = useRef(null)
  const inputRef = useRef(null)

  // 暴露填入方法给父组件
  useEffect(() => {
    if (fillToInputRef) {
      fillToInputRef.current = (text) => {
        if (inputRef.current && text) {
          const textarea = inputRef.current.querySelector('textarea')
          if (textarea) {
            // 使用 nativeInputValueSetter 设置值
            const nativeSetter = Object.getOwnPropertyDescriptor(
              window.HTMLTextAreaElement.prototype, 'value'
            ).set
            nativeSetter.call(textarea, text)
            textarea.dispatchEvent(new Event('input', { bubbles: true }))
            textarea.focus()
          }
        }
      }
    }
  }, [fillToInputRef])

  // 暴露清空附件的方法给父组件
  useEffect(() => {
    if (clearAttachmentsRef) {
      clearAttachmentsRef.current = () => {
        attachments.forEach(a => {
          if (a.previewUrl) URL.revokeObjectURL(a.previewUrl)
        })
        setAttachments([])
        onAttachmentsChange?.([])
      }
    }
  }, [attachments, clearAttachmentsRef, onAttachmentsChange])

  // 处理文件选择 - 不再转换为 base64，只保存文件对象
  const handleFileSelect = useCallback(async (e) => {
    const files = Array.from(e.target.files || [])
    if (files.length === 0) return

    const newAttachments = files.map(file => ({
      id: `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      file,
      previewUrl: file.type?.startsWith('image/') ? URL.createObjectURL(file) : null,
      // 不再在这里转换 base64
    }))

    setAttachments(prev => {
      const updated = [...prev, ...newAttachments]
      // 只通知文件元信息，不包含 base64
      onAttachmentsChange?.(updated.map(a => ({
        name: a.file.name,
        type: a.file.type,
        size: a.file.size,
        file: a.file, // 传递 File 对象用于上传
      })))
      return updated
    })

    // 清空 input 以便可以重复选择同一文件
    if (fileInputRef.current) {
      fileInputRef.current.value = ''
    }
  }, [onAttachmentsChange])

  // 移除附件
  const removeAttachment = useCallback((id) => {
    setAttachments(prev => {
      const target = prev.find(a => a.id === id)
      if (target?.previewUrl) {
        URL.revokeObjectURL(target.previewUrl)
      }
      const updated = prev.filter(a => a.id !== id)
      // 通知父组件附件变化
      onAttachmentsChange?.(updated.map(a => ({
        name: a.file.name,
        type: a.file.type,
        size: a.file.size,
        data: a.data,
      })))
      return updated
    })
  }, [onAttachmentsChange])

  // 清理预览 URL
  useEffect(() => {
    return () => {
      attachments.forEach(a => {
        if (a.previewUrl) URL.revokeObjectURL(a.previewUrl)
      })
    }
  }, [])

  // 打开文件选择器
  const openFilePicker = useCallback(() => {
    fileInputRef.current?.click()
  }, [])

  // 预览附件
  const handlePreview = async (attachment) => {
    if (!attachment?.file) return

    // 创建预览URL
    let previewUrl = attachment.previewUrl
    if (!previewUrl) {
      previewUrl = URL.createObjectURL(attachment.file)
    }

    setPreviewModal({
      open: true,
      attachment: {
        ...attachment,
        previewUrl,
        name: attachment.file.name,
        type: attachment.file.type,
        size: attachment.file.size
      },
      content: null,
    })

    // 文本文件：加载内容并检测是否为 markdown
    const ext = (attachment.file.name?.split('.').pop() || '').toLowerCase()
    const textExts = ['txt', 'md', 'json', 'xml', 'html', 'css', 'js', 'py', 'java', 'cpp', 'c', 'h', 'go', 'rs', 'ts', 'jsx', 'tsx']
    if (textExts.includes(ext)) {
      try {
        const text = await attachment.file.text()
        setPreviewModal(prev => ({ ...prev, content: text }))
      } catch (e) {
        console.error('加载文本文件失败:', e)
      }
    }
  }

  // 关闭预览
  const closePreview = () => {
    setPreviewModal({ open: false, attachment: null, content: null })
  }

  // 判断文件类型
  const getExt = (name) => (name?.split('.').pop() || '').toLowerCase()
  const isImage = (file) => file?.type?.startsWith('image/')
  const isPDF = (file) => getExt(file?.name) === 'pdf' || file?.type === 'application/pdf'
  const isExcel = (file) => ['xlsx', 'xls'].includes(getExt(file?.name))
  const isDocx = (file) => ['docx', 'doc'].includes(getExt(file?.name))
  const isCSV = (file) => getExt(file?.name) === 'csv' || file?.type === 'text/csv'
  const isText = (file) => ['txt', 'md', 'json', 'xml', 'html', 'css', 'js', 'py', 'java', 'cpp', 'c', 'h', 'go', 'rs', 'ts', 'jsx', 'tsx'].includes(getExt(file?.name))
  const isAuPage = (file) => file?.name?.endsWith('.aupage.json') || file?.name?.endsWith('.aupage')

  return (
    <ComposerPrimitive.Root
      ref={inputRef}
      className="mx-auto w-full max-w-[--thread-max-width] rounded-2xl border-2 border-border bg-white shadow-lg"
    >
      {/* 附件预览区域 */}
      {attachments.length > 0 && (
        <div className="px-4 pt-3 pb-0">
          <div className="flex flex-wrap gap-2">
            {attachments.map((attachment) => (
              <AttachmentPreview
                key={attachment.id}
                attachment={attachment}
                onRemove={() => removeAttachment(attachment.id)}
                onPreview={handlePreview}
              />
            ))}
          </div>
        </div>
      )}

      <div className="flex items-end gap-2 p-4">
        {/* 附件按钮 */}
        <button
          type="button"
          onClick={openFilePicker}
          disabled={disabled}
          className="h-10 w-10 rounded-full hover:bg-muted transition-colors flex items-center justify-center text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed"
          title="添加图片或文件"
        >
          <Paperclip className="h-5 w-5" />
        </button>

        <ComposerPrimitive.Input
          rows={1}
          disabled={disabled}
          className="flex-1 border-0 bg-transparent px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
          placeholder="输入消息... (Enter 发送, Shift+Enter 换行)"
        />
        <ComposerPrimitive.Send
          className="h-10 w-10 rounded-full bg-primary text-primary-foreground hover:bg-primary/90 transition-all hover:scale-105 flex items-center justify-center disabled:opacity-50 disabled:cursor-not-allowed shadow-md"
        >
          <ArrowUp className="h-5 w-5" />
        </ComposerPrimitive.Send>
      </div>

      {/* 隐藏的文件输入 */}
      <input
        ref={fileInputRef}
        type="file"
        multiple
        accept="image/*,.pdf,.doc,.docx,.txt,.md,.json,.csv,.xlsx,.xls"
        onChange={handleFileSelect}
        className="hidden"
      />

      {/* 预览模态框 */}
      {previewModal.open && previewModal.attachment && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4" onClick={closePreview}>
          <div className={`bg-background border border-border rounded-lg w-full max-h-[90vh] overflow-hidden shadow-xl ${isExcel(previewModal.attachment) ? 'max-w-[95vw] lg:max-w-[90vw]' : 'max-w-5xl'}`} onClick={(e) => e.stopPropagation()}>
            {/* 头部 */}
            <div className="flex items-center justify-between p-4 border-b border-border">
              <h3 className="text-lg font-semibold truncate">{previewModal.attachment.name}</h3>
              <button
                onClick={closePreview}
                className="btn btn-ghost btn-sm btn-circle"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            {/* 内容区域 */}
            <div className="p-4 overflow-auto max-h-[calc(90vh-80px)]">
              {/* 图片预览 */}
              {isImage(previewModal.attachment) && (
                <img
                  src={previewModal.attachment.previewUrl}
                  alt={previewModal.attachment.name}
                  className="max-w-full h-auto mx-auto"
                />
              )}

              {/* PDF预览 */}
              {isPDF(previewModal.attachment) && (
                <div className="w-full h-[70vh] border border-border rounded overflow-hidden">
                  <iframe
                    src={`${previewModal.attachment.previewUrl}#toolbar=0&navpanes=0`}
                    className="w-full h-full"
                    title="PDF 预览"
                  />
                </div>
              )}

              {/* CSV预览 */}
              {isCSV(previewModal.attachment) && (
                <iframe
                  src={previewModal.attachment.previewUrl}
                  title="csv-preview"
                  className="w-full h-[70vh] bg-muted border border-border rounded"
                />
              )}

              {/* Excel预览 */}
              {isExcel(previewModal.attachment) && (
                <div className="h-[70vh] min-h-0 w-full">
                  <ExcelPreview
                    blob={previewModal.attachment.file}
                    filename={previewModal.attachment.name}
                    className="h-full"
                  />
                </div>
              )}

              {/* Word 文档预览 */}
              {isDocx(previewModal.attachment) && (
                <div className="h-[70vh] min-h-0">
                  <DocxPreview
                    blob={previewModal.attachment.file}
                    filename={previewModal.attachment.name}
                    className="h-full"
                  />
                </div>
              )}

              {/* AuPage 文档预览 */}
              {isAuPage(previewModal.attachment) && (
                <div className="h-full">
                  <AuPageFileLoader
                    url={previewModal.attachment.previewUrl}
                  />
                </div>
              )}

              {/* 文本文件预览 */}
              {isText(previewModal.attachment) && (
                <div className="relative max-h-[70vh] overflow-auto">
                  {previewModal.content === null ? (
                    <div className="flex justify-center py-8">
                      <span className="loading loading-spinner loading-lg"></span>
                    </div>
                  ) : isMarkdownContent(previewModal.content, previewModal.attachment.name) ? (
                    <SimpleMarkdownPreview content={previewModal.content} fileName={previewModal.attachment.name} />
                  ) : (
                    <pre className="whitespace-pre-wrap break-words text-sm bg-muted rounded p-4 font-mono border border-border">
                      {previewModal.content || '无法加载文件内容'}
                    </pre>
                  )}
                </div>
              )}

              {/* 不支持的类型 */}
              {!isImage(previewModal.attachment) && !isPDF(previewModal.attachment) && !isCSV(previewModal.attachment) && !isExcel(previewModal.attachment) && !isDocx(previewModal.attachment) && !isText(previewModal.attachment) && !isAuPage(previewModal.attachment) && (
                <div className="text-center py-8">
                  <div className="text-muted-foreground mb-4">
                    <p>暂不支持该文件类型的在线预览。</p>
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </ComposerPrimitive.Root>
  )
}

export function ChatThread({ threadId, onThreadCreated }) {
  const { productName } = useAppConfig()
  const [quickPrompts, setQuickPrompts] = useState([])

  // 从 store 读取当前 thread 的全部状态，组件销毁时数据仍在 store 中
  const defaultThread = useMemo(() => ({
    messages: [],
    isRunning: false,
    stepStatuses: {},
    streamingMessageId: null,
    isQueued: false,
    queuePosition: 0,
    startedAt: null,
    isLoading: false,
  }), [])
  const thread = useThreadStore(useShallow((s) =>
    s.threads[threadId] ?? defaultThread
  ))

  const {
    messages,
    isRunning,
    stepStatuses,
    streamingMessageId,
    isQueued,
    queuePosition,
  } = thread

  // 写回 store 的 setMessages（供 useExternalStoreRuntime 使用）
  const setMessages = useCallback((msgsOrUpdater) => {
    if (!threadId) return
    const prev = useThreadStore.getState().threads[threadId]?.messages ?? []
    const next = typeof msgsOrUpdater === 'function' ? msgsOrUpdater(prev) : msgsOrUpdater
    useThreadStore.getState().setThreadMessages(threadId, next)
  }, [threadId])

  const pendingAttachmentsRef = useRef([])
  const clearAttachmentsRef = useRef(null)
  const fillToInputRef = useRef(null)
  const viewportRef = useRef(null)
  const { isReady: inferenceReady, processState } = useInferenceHealthContext()
  // Only show "LLM 推理服务正在启动中" while the backend is actually starting
  // (starting/warming_up/restarting). If the inference process is stopped, the
  // LLM simply isn't running — don't mislead with a "starting" banner.
  const startingStates = ['starting', 'warming_up', 'restarting']
  const isInferenceStarting = startingStates.includes(String(processState || '').toLowerCase())
  const inferenceNotReady = isInferenceStarting
  const { identityId } = useAuth()

  // 初始化 thread：检查 running 状态、加载历史、重连 SSE（逻辑在 store 中）
  useEffect(() => {
    if (!threadId) return
    useThreadStore.getState().initThread(threadId)
    // 组件卸载时中止该 thread 的 SSE 连接，释放浏览器连接槽位
    return () => {
      useThreadStore.getState().abortThread(threadId)
    }
  }, [threadId])

  // ========== 发送消息（委托给 store） ==========
  const onNew = useCallback(async (message) => {
    const userText = message.content?.[0]?.text || ''
    if (!userText) return

    useThreadStore.getState().sendMessage(threadId, {
      text: userText,
      identityId,
      attachments: pendingAttachmentsRef.current,
      onThreadCreated,
      onClearAttach: () => clearAttachmentsRef.current?.(),
      onFillToInput: (text) => fillToInputRef.current?.(text),
      inferenceReady,
    })
  }, [threadId, identityId, onThreadCreated, inferenceReady])

  // ========== 消息转换 ==========
  const convertMessage = useCallback((message) => {
    return { role: message.role, content: message.content, id: message.id, createdAt: message.createdAt, metadata: message.metadata }
  }, [])

  const runtime = useExternalStoreRuntime({ messages, setMessages, isRunning, convertMessage, onNew })

  // ========== 建议点击 ==========
  const handleSuggestionClick = useCallback((text) => {
    if (runtime && text) {
      runtime.thread.append({ role: 'user', content: [{ type: 'text', text }] })
    }
  }, [runtime])

  // ========== 附件变化回调 ==========
  const handleAttachmentsChange = useCallback((attachments) => {
    pendingAttachmentsRef.current = attachments || []
  }, [])



  const fillToInput = useCallback((text) => {
    if (fillToInputRef.current) fillToInputRef.current(text)
  }, [])


  useEffect(() => {
    fetch('/api/quick-prompts/public')
      .then(res => res.json())
      .then(data => setQuickPrompts(data.prompts || []))
      .catch(err => console.error('加载快捷提示失败:', err))
  }, [])

  const renderIcon = (iconName) => {
    const iconMap = {
      Sparkles: <Sparkles className="w-5 h-5" />,
      Code: <Code className="w-5 h-5" />,
      FileQuestion: <FileQuestion className="w-5 h-5" />,
      Lightbulb: <Lightbulb className="w-5 h-5" />,
    }
    return iconMap[iconName] || <Sparkles className="w-5 h-5" />
  }

  return (
    <AssistantRuntimeProvider runtime={runtime}>
    <ThreadIdContext.Provider value={threadId}>
    <FillToInputContext.Provider value={fillToInput}>
    <StepStatusContext.Provider value={stepStatuses}>
    <StreamingMessageIdContext.Provider value={streamingMessageId}>
    <ThreadPrimitive.Root
      className="flex h-full flex-col bg-background"
      style={{ '--thread-max-width': '64rem' }}
    >
      <ThreadPrimitive.Viewport
        ref={viewportRef}
        className="relative flex flex-1 flex-col overflow-x-auto overflow-y-scroll px-4 pt-4"
      >
        {thread.isLoading && (
          <div className="absolute inset-0 flex flex-col items-center justify-center bg-background z-50">
            <div className="animate-spin rounded-full h-10 w-10 border-b-2 border-primary mb-4"></div>
            <p className="text-muted-foreground text-sm">正在加载历史消息...</p>
          </div>
        )}
{!thread.isLoading && (
          <>
            <ThreadPrimitive.If empty>
              <div className="flex flex-col items-center justify-center h-full text-center space-y-6 py-12">
                <div className="w-16 h-16 rounded-full bg-primary/10 flex items-center justify-center">
                  <Bot className="w-8 h-8 text-primary" />
                </div>
                <div className="space-y-2">
                  <h2 className="text-3xl font-bold">
                    <span className="text-muted-foreground">欢迎使用</span>
                    <span className="text-primary ml-2">{productName}</span>
                  </h2>
                  <p className="text-muted-foreground text-lg max-w-md mt-2">
                    我是一个智能助手，可以帮助你完成各种任务
                  </p>
                </div>
                {quickPrompts.length > 0 && (
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 w-full max-w-2xl mt-4">
                    {quickPrompts.map((prompt) => (
                      <SuggestionCard
                        key={prompt.id}
                        icon={renderIcon(prompt.icon)}
                        title={prompt.title}
                        description={prompt.description}
                        onClick={() => handleSuggestionClick(prompt.prompt)}
                      />
                    ))}
                  </div>
                )}
              </div>
            </ThreadPrimitive.If>
            <ThreadPrimitive.Messages
              components={{
                UserMessage: SafeMessageWithEditComposer,
                AssistantMessage: SafeMessageWithEditComposer,
              }}
            />
          </>
        )}

        <div className="h-24 shrink-0" />

        <div className="sticky bottom-0 mx-auto mt-auto flex w-full max-w-[--thread-max-width] flex-col gap-4 overflow-visible rounded-t-2xl bg-background pb-4 pt-2">
          {inferenceNotReady && (
            <div className="mx-auto w-full px-3 py-2 rounded-lg bg-amber-50 border border-amber-200 text-amber-700 text-sm flex items-center gap-2">
              <Loader2 className="h-4 w-4 shrink-0 animate-spin" />
              <span>LLM 推理服务正在启动中，请稍候...</span>
            </div>
          )}
          {isQueued && !thread.isLoading && <QueueStatus position={queuePosition} />}
          <ThreadScrollToBottom />
          <Composer onAttachmentsChange={handleAttachmentsChange} clearAttachmentsRef={clearAttachmentsRef} fillToInputRef={fillToInputRef} disabled={inferenceNotReady || isRunning} />
        </div>
      </ThreadPrimitive.Viewport>
    </ThreadPrimitive.Root>
    </StreamingMessageIdContext.Provider>
    </StepStatusContext.Provider>
    </FillToInputContext.Provider>
    </ThreadIdContext.Provider>
    </AssistantRuntimeProvider>
  )
}
