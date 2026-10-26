import { useEffect, useRef, useState } from 'react'
import { renderAsync } from 'docx-preview'
import JSZip from 'jszip'

function DocxPreview({ src, blob, arrayBuffer, filename = '', className = '' } = {}) {
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const containerRef = useRef(null)
  const loadReqIdRef = useRef({ v: 0 })

  useEffect(() => {
    if (!src && !blob && !arrayBuffer) {
      setError('')
      setLoading(false)
      return
    }

    // 提前检查文件名扩展名
    const fileName = filename || (src ? String(src).split('/').pop() || '' : '')
    const ext = fileName.split('.').pop()?.toLowerCase()
    
    // .doc 格式提示（但不阻止，因为可能是扩展名错误）
    if (ext === 'doc') {
      console.log('File has .doc extension, will attempt to parse as docx')
    }

    const rid = (loadReqIdRef.current.v += 1)
    setLoading(true)
    setError('')

    ;(async () => {
      try {
        let data = arrayBuffer
        if (!data && blob) {
          data = await blob.arrayBuffer()
        }
        if (!data) {
          const token = localStorage.getItem('token')
          const headers = {}
          if (token) headers.Authorization = `Bearer ${token}`
          const resp = await fetch(String(src), {
            credentials: 'include',
            headers,
          })
          if (!resp.ok) throw new Error(resp.statusText || '下载失败')
          data = await resp.arrayBuffer()
        }
        if (loadReqIdRef.current.v !== rid) return

        // 等待一帧确保 DOM 已更新
        await new Promise(resolve => requestAnimationFrame(resolve))
        
        const container = containerRef.current
        if (!container) {
          throw new Error('渲染容器不可用')
        }
        
        // 清空容器内容，避免重复渲染
        container.innerHTML = ''

        // 验证数据是否有效
        if (!data || data.byteLength === 0) {
          throw new Error('文件数据为空')
        }

        // 验证 docx 文件结构是否有效
        try {
          const zip = await JSZip.loadAsync(data)
          // 检查是否包含必需的 word/document.xml 文件
          if (!zip.file('word/document.xml')) {
            // 可能是旧的 .doc 格式或其他格式
            const isDocExt = ext === 'doc'
            throw new Error(
              isDocExt 
                ? '此文件是 .doc 格式，不是 .docx 格式。请用 Word 打开后另存为 .docx 格式'
                : '此文件不是有效的 .docx 格式'
            )
          }
          // 如果 .doc 文件包含 word/document.xml，说明它实际上是 docx 格式
          if (ext === 'doc') {
            console.log('File has .doc extension but contains docx structure, will render normally')
          }
        } catch (zipError) {
          // JSZip 验证失败，直接抛出友好的错误信息
          if (zipError.message.includes('不是有效的') || zipError.message.includes('请用 Word 打开')) {
            throw zipError
          }
          // 其他 ZIP 解析错误
          const isDocExt = ext === 'doc'
          throw new Error(
            isDocExt
              ? '⚠️ 此 .doc 文件不是 ZIP 格式（可能是旧的 .doc 二进制格式或文件已损坏）。\n\n解决方案：\n1. 用 Word 或 WPS 打开此文件\n2. 另存为 ".docx" 格式\n3. 重新上传'
              : '文件不是有效的 ZIP 格式（.docx 文件应该是 ZIP 压缩包）。文件可能已损坏。'
          )
        }

        await renderAsync(data, container, container, {
          className: 'docx-wrapper',
          inWrapper: true,
          ignoreWidth: false,
          ignoreHeight: false,
          ignoreFonts: false,
          breakPages: true,
          debug: false,
        })

        if (loadReqIdRef.current.v !== rid) return
      } catch (e) {
        if (loadReqIdRef.current.v !== rid) return
        console.error('DocxPreview render error:', e)
        
        // 提供更友好的错误信息
        let errorMsg = e?.message || String(e || '解析失败')
        
        // 检测特定错误类型并提供针对性建议
        if (errorMsg.includes('childNodes') || errorMsg.includes('null')) {
          errorMsg = '文档格式不兼容或已损坏，无法预览。建议：1) 尝试用 Word 打开并另存为新文件 2) 下载后查看'
        } else if (errorMsg.includes('不是 ZIP 格式') || errorMsg.includes('二进制格式')) {
          // 真正的 .doc 二进制格式
          if (!errorMsg.includes('⚠️')) {
            errorMsg = '⚠️ 此文件是旧的 .doc 二进制格式，不支持在线预览。\n\n解决方案：\n1. 用 Microsoft Word 或 WPS 打开此文件\n2. 点击"文件" → "另存为"\n3. 选择格式为 ".docx"\n4. 重新上传新文件'
          }
        }
        // 其他错误保持原样（包括扩展名不匹配的情况）
        
        setError(errorMsg)
      } finally {
        if (loadReqIdRef.current.v !== rid) return
        setLoading(false)
      }
    })()
  }, [src, blob, arrayBuffer])

  const title = filename || (src ? String(src).split('/').pop() || '' : '')

  if (!src && !blob && !arrayBuffer) {
    return <div className={`text-sm text-muted-foreground ${className}`}>暂无预览内容</div>
  }

  return (
    <div className={`flex flex-col min-h-0 ${className}`}>
      <div className="flex items-center justify-between gap-2 mb-2 flex-wrap">
        <div className="min-w-0">
          <div className="text-sm font-semibold truncate" title={title}>
            {title || 'Word 文档'}
          </div>
          <div className="text-xs text-muted-foreground">Word 文档预览</div>
        </div>
      </div>

      {/* loading 和 error 提示覆盖在容器上方 */}
      {loading && (
        <div className="py-16 flex justify-center">
          <span className="loading loading-spinner loading-lg"></span>
        </div>
      )}
      {error && (
        <div className="text-sm text-destructive break-words px-2 whitespace-pre-wrap">{error}</div>
      )}
      {/* 容器始终渲染，docx-preview 需要实际存在的 DOM 节点 */}
      <div
        className="flex-1 min-h-0 border border-border rounded bg-background overflow-auto p-4"
      >
        <div ref={containerRef} className="docx-container" />
      </div>
    </div>
  )
}

export default DocxPreview
