import React, { useState, useEffect, useRef } from 'react';
import { Loader2, AlertCircle, AlertTriangle } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { extname, formatSize } from '../lib/utils.js';

// 大文件阈值：5MB（参照 dufs PreviewDialog/OfficePreview 的限制）
const LARGE_FILE_THRESHOLD = 5 * 1024 * 1024;

// 超过 5MB 的 Office 文件不做解析，直接提示下载
function LargeFileNotice({ size }) {
  const { t } = useTranslation();
  return (
    <div className="py-8 flex flex-col items-center justify-center gap-3 text-center">
      <AlertTriangle size={40} className="text-amber-500 mx-auto" />
      <p className="text-base font-medium text-amber-600 dark:text-amber-400">
        {t('officePreviewLargeFile', { defaultValue: '文件过大，无法预览' })}
      </p>
      <p className="text-sm text-gray-500 dark:text-gray-400">
        {t('officePreviewLargeFileDesc', {
          defaultValue: '文件大小 {{size}}，超过 5MB 预览限制，请下载后查看。',
          size: formatSize(size),
        })}
      </p>
    </div>
  );
}

function LoadingSpinner({ text }) {
  return (
    <div className="py-12 flex flex-col items-center justify-center gap-3">
      <Loader2 size={28} className="animate-spin text-gray-400" />
      <p className="text-sm text-gray-400">{text}</p>
    </div>
  );
}

function ErrorNotice({ message }) {
  return (
    <div className="py-8 flex flex-col items-center justify-center gap-3 text-center">
      <AlertCircle size={36} className="text-red-500 mx-auto" />
      <p className="text-sm text-red-500">{message}</p>
    </div>
  );
}

/**
 * 文档预览骨架：管理 loading / error / 大文件拦截，并把容器 ref 交给子渲染逻辑。
 * @param {object}   props
 * @param {string}   props.url     文件地址
 * @param {number}   props.size    文件字节数（可选，用于 >5MB 拦截）
 * @param {Function} props.onLoad  就绪回调（成功/失败/过大都会调用，用于关闭外层 spinner）
 * @param {Function} props.onError 失败回调（可选，向后兼容）
 * @param {Function} props.render  (containerRef) => void，负责把内容渲染进容器
 * @param {string}   props.containerClassName 容器 class
 */
function PreviewShell({ url, size, onLoad, onError, render, containerClassName = '' }) {
  const { t } = useTranslation();
  const containerRef = useRef(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const tooLarge = size != null && size > LARGE_FILE_THRESHOLD;

  useEffect(() => {
    if (tooLarge) {
      setLoading(false);
      setError(null);
      onLoad?.();
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);

    Promise.resolve()
      .then(() => render(containerRef.current))
      .then(() => {
        if (!cancelled) {
          setLoading(false);
          onLoad?.();
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setLoading(false);
          setError(err?.message || t('officePreviewError', { defaultValue: '文档加载失败' }));
          onLoad?.();
          onError?.(err);
        }
      });

    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [url, size, tooLarge]);

  if (tooLarge) {
    return <LargeFileNotice size={size} />;
  }

  return (
    <div className="w-full">
      {loading && <LoadingSpinner text={t('officePreviewLoading', { defaultValue: '文档加载中...' })} />}
      {error && <ErrorNotice message={error} />}
      <div
        ref={containerRef}
        className={containerClassName}
        style={{ display: loading || error ? 'none' : undefined }}
      />
    </div>
  );
}

// DOCX preview using docx-preview
function DocxPreview({ url, size, onLoad, onError }) {
  return (
    <PreviewShell
      url={url}
      size={size}
      onLoad={onLoad}
      onError={onError}
      containerClassName="docx-preview prose prose-sm max-w-none dark:prose-invert p-4"
      render={(container) =>
        import('docx-preview').then((docxPreview) =>
          fetch(url)
            .then((r) => {
              if (!r.ok) throw new Error(`Failed to fetch: ${r.status}`);
              return r.arrayBuffer();
            })
            .then((buf) =>
              docxPreview.renderAsync(buf, container, undefined, {
                className: 'docx-preview',
                inWrapper: true,
                ignoreWidth: false,
                ignoreHeight: false,
                ignoreFonts: false,
                breakPages: true,
                ignoreLastRenderedPageBreak: true,
                experimental: false,
                trimXmlDeclaration: true,
                useBase64URL: false,
                renderChanges: false,
                renderHeaders: true,
                renderFooters: true,
                renderFootnotes: true,
                renderEndnotes: true,
              })
            )
        )
      }
    />
  );
}

// XLSX preview using xlsx (SheetJS)
function XlsxPreview({ url, size, onLoad, onError }) {
  const { t } = useTranslation();
  const containerRef = useRef(null);
  const [sheetName, setSheetName] = useState('');
  const [sheets, setSheets] = useState([]);
  const wbRef = useRef(null);
  const XLSXRef = useRef(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const tooLarge = size != null && size > LARGE_FILE_THRESHOLD;

  useEffect(() => {
    if (tooLarge) {
      setLoading(false);
      setError(null);
      onLoad?.();
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);

    import('xlsx')
      .then((XLSX) => {
        XLSXRef.current = XLSX;
        return fetch(url);
      })
      .then((r) => {
        if (!r.ok) throw new Error(`Failed to fetch: ${r.status}`);
        return r.arrayBuffer();
      })
      .then((buf) => {
        if (cancelled) return;
        const wb = XLSXRef.current.read(buf, { type: 'array' });
        wbRef.current = wb;
        setSheets(wb.SheetNames);
        if (wb.SheetNames.length > 0) {
          setSheetName(wb.SheetNames[0]);
          renderSheet(wb, wb.SheetNames[0]);
        }
      })
      .then(() => {
        if (!cancelled) {
          setLoading(false);
          onLoad?.();
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setLoading(false);
          setError(err?.message || t('officePreviewError', { defaultValue: '文档加载失败' }));
          onLoad?.();
          onError?.(err);
        }
      });

    function renderSheet(wb, name) {
      if (!containerRef.current) return;
      const ws = wb.Sheets[name];
      const html = XLSXRef.current.utils.sheet_to_html(ws, { editable: false });
      containerRef.current.innerHTML = html;
      const table = containerRef.current.querySelector('table');
      if (table) {
        table.className = 'w-full border-collapse text-xs';
        table.querySelectorAll('td, th').forEach((cell) => {
          cell.className = 'border border-gray-300 dark:border-gray-600 px-2 py-1';
        });
      }
    }

    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [url, size, tooLarge]);

  function switchSheet(name) {
    setSheetName(name);
    if (wbRef.current && XLSXRef.current) {
      const ws = wbRef.current.Sheets[name];
      const html = XLSXRef.current.utils.sheet_to_html(ws, { editable: false });
      if (containerRef.current) containerRef.current.innerHTML = html;
    }
  }

  if (tooLarge) {
    return <LargeFileNotice size={size} />;
  }

  return (
    <div className="w-full">
      {loading && <LoadingSpinner text={t('officePreviewLoading', { defaultValue: '文档加载中...' })} />}
      {error && <ErrorNotice message={error} />}
      {!loading && !error && sheets.length > 1 && (
        <div className="flex gap-1 mb-2 border-b border-gray-200 dark:border-gray-700 pb-2">
          {sheets.map((s) => (
            <button
              key={s}
              onClick={() => switchSheet(s)}
              className={`px-2 py-1 text-xs rounded ${s === sheetName ? 'bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300' : 'hover:bg-gray-100 dark:hover:bg-gray-800'}`}
            >
              {s}
            </button>
          ))}
        </div>
      )}
      <div ref={containerRef} className="overflow-auto max-h-[70vh]" style={{ display: loading || error ? 'none' : undefined }} />
    </div>
  );
}

// PPTX preview using PptxViewJS (canvas rendering, replaces pptx2html)
function PptxPreview({ url, size, onLoad, onError }) {
  const { t } = useTranslation();
  const viewerRef = useRef(null);

  // Destroy the viewer when the component unmounts
  useEffect(() => {
    return () => {
      viewerRef.current?.destroy?.();
      viewerRef.current = null;
    };
  }, []);

  return (
    <PreviewShell
      url={url}
      size={size}
      onLoad={onLoad}
      onError={onError}
      containerClassName="pptx-preview-canvas w-full"
      render={async (container) => {
        // Tear down a previous viewer (URL changed) before re-rendering
        viewerRef.current?.destroy?.();
        viewerRef.current = null;
        container.innerHTML = '';

        // Canvas element PptxViewJS renders into
        const canvas = document.createElement('canvas');
        canvas.style.width = '100%';
        canvas.style.maxHeight = '70vh';
        container.appendChild(canvas);

        const { PPTXViewer } = await import('pptxviewjs');
        const viewer = new PPTXViewer({
          canvas,
          slideSizeMode: 'fit',
          backgroundColor: '#ffffff',
        });
        viewerRef.current = viewer;

        await viewer.loadFromUrl(url);
        await viewer.render();

        // Slide navigation controls
        const nav = document.createElement('div');
        nav.className = 'flex items-center justify-center gap-3 mt-3';
        const btnClass =
          'px-3 py-1 text-xs rounded border border-gray-300 dark:border-gray-600 ' +
          'hover:bg-gray-100 dark:hover:bg-gray-800 text-gray-700 dark:text-gray-200 disabled:opacity-40 disabled:cursor-not-allowed';

        const prevBtn = document.createElement('button');
        prevBtn.className = btnClass;
        prevBtn.textContent = t('pptxPrevSlide', { defaultValue: '上一页' });
        const nextBtn = document.createElement('button');
        nextBtn.className = btnClass;
        nextBtn.textContent = t('pptxNextSlide', { defaultValue: '下一页' });

        const pageLabel = document.createElement('span');
        pageLabel.className = 'text-xs text-gray-500 dark:text-gray-400 tabular-nums';

        const updateNav = () => {
          const current = viewer.getCurrentSlideIndex();
          const total = viewer.getSlideCount();
          pageLabel.textContent = `${current + 1} / ${total}`;
          prevBtn.disabled = current <= 0;
          nextBtn.disabled = current >= total - 1;
        };

        prevBtn.addEventListener('click', async () => {
          await viewer.previousSlide();
          updateNav();
        });
        nextBtn.addEventListener('click', async () => {
          await viewer.nextSlide();
          updateNav();
        });

        nav.appendChild(prevBtn);
        nav.appendChild(pageLabel);
        nav.appendChild(nextBtn);
        container.appendChild(nav);
        updateNav();
      }}
    />
  );
}

// PDF preview (basic - browser native or link)
function PdfPreview({ url, onLoad, onError }) {
  return (
    <iframe
      src={url}
      className="w-full"
      style={{ height: '70vh', border: 'none' }}
      onLoad={onLoad}
      onError={onError}
      title="PDF Preview"
    />
  );
}

/**
 * Office 文件预览入口。
 * Props: { url, filename, size?, onLoad? }
 * - size 可选：>5MB 的 docx/xlsx/pptx 直接提示"文件过大，无法预览"
 * - 加载中/失败都有内置状态展示；就绪后统一调用 onLoad
 */
export function OfficePreview({ url, filename, size, onLoad, onError }) {
  const { t } = useTranslation();
  const ext = extname(filename);

  switch (ext) {
    case 'docx':
      return <DocxPreview url={url} size={size} onLoad={onLoad} onError={onError} />;
    case 'doc':
      return <DocxPreview url={url} size={size} onLoad={onLoad} onError={onError} />;
    case 'xlsx':
      return <XlsxPreview url={url} size={size} onLoad={onLoad} onError={onError} />;
    case 'xls':
      return <XlsxPreview url={url} size={size} onLoad={onLoad} onError={onError} />;
    case 'pptx':
      return <PptxPreview url={url} size={size} onLoad={onLoad} onError={onError} />;
    case 'ppt':
      return <PptxPreview url={url} size={size} onLoad={onLoad} onError={onError} />;
    case 'pdf':
      return <PdfPreview url={url} onLoad={onLoad} onError={onError} />;
    default:
      return (
        <div className="text-center text-sm text-gray-500 p-8">
          <p>{t('officePreviewUnavailable', { defaultValue: '该文件类型暂不支持预览' })}</p>
          <a href={url} download={filename} className="inline-block mt-2 text-blue-600 hover:underline text-xs">
            {t('download', { defaultValue: '下载' })}
          </a>
        </div>
      );
  }
}
