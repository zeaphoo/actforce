import React, { useState, useEffect, useRef } from 'react';
import {
  X, Download, Loader, File, ChevronLeft, ChevronRight, Music, AlertTriangle,
} from 'lucide-react';
import { extname, isTextFile, cn, formatSize } from '../lib/utils.js';
import { useTranslation } from 'react-i18next';
import { OfficePreview } from './OfficePreview.jsx';
import { MarkdownRenderer, CODE_LANG_BY_EXT, buildCodeBlockHtml, applyCodeHighlight } from './MarkdownRenderer.jsx';
import {
  Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription,
} from './ui/dialog.jsx';
import { Button } from './ui/button.jsx';
import { Input } from './ui/input.jsx';
import { Slider } from './ui/slider.jsx';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select.jsx';
import { Switch } from './ui/switch.jsx';

// ---------------------------------------------------------------------------
// 预览类型分派（按扩展名，参照 dufs PreviewDialog 的 getPreviewMode 思路，
// 但完全基于 actforce 自己的扩展名集合实现）
// ---------------------------------------------------------------------------
const IMAGE_EXTS = new Set(['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp', 'ico', 'avif']);
const VIDEO_EXTS = new Set(['mp4', 'webm', 'ogg', 'mov', 'avi', 'mkv', 'ogv']);
const AUDIO_EXTS = new Set(['mp3', 'wav', 'flac', 'aac', 'ogg', 'opus', 'm4a', 'oga', 'weba']);
const FONT_EXTS = new Set(['ttf', 'ttc', 'otf', 'otc', 'woff', 'woff2']);
const OFFICE_EXTS = new Set(['docx', 'xlsx', 'pptx']);
const MARKDOWN_EXTS = new Set(['md', 'markdown']);

// 旧版二进制 Office 格式（doc/xls/ppt）无法在前端解析，直接提示下载
const LEGACY_OFFICE_EXTS = new Set(['doc', 'xls', 'ppt']);

function getPreviewMode(filename) {
  const ext = extname(filename);
  if (IMAGE_EXTS.has(ext)) return 'image';
  if (VIDEO_EXTS.has(ext)) return 'video';
  if (AUDIO_EXTS.has(ext)) return 'audio';
  if (ext === 'pdf') return 'pdf';
  if (FONT_EXTS.has(ext)) return 'font';
  if (OFFICE_EXTS.has(ext)) return 'office';
  if (MARKDOWN_EXTS.has(ext)) return 'markdown';
  if (LEGACY_OFFICE_EXTS.has(ext)) return 'unsupported';
  if (CODE_LANG_BY_EXT[ext] || isTextFile(filename)) return 'text';
  return 'unsupported';
}

// 字体格式 → @font-face format
const FONT_FORMATS = {
  ttf: 'truetype',
  ttc: 'truetype',
  otf: 'opentype',
  otc: 'opentype',
  woff: 'woff',
  woff2: 'woff2',
};

// 字重选项（对齐 dufs PreviewDialog 的 FontPreview）
const FONT_WEIGHTS = [
  { value: '100', label: '100 (Thin)' },
  { value: '200', label: '200 (Extra Light)' },
  { value: '300', label: '300 (Light)' },
  { value: '400', label: '400 (Regular)' },
  { value: '500', label: '500 (Medium)' },
  { value: '600', label: '600 (Semi Bold)' },
  { value: '700', label: '700 (Bold)' },
  { value: '800', label: '800 (Extra Bold)' },
  { value: '900', label: '900 (Black)' },
];

/**
 * 字体预览：把字体文件 fetch 成 blob，用 URL.createObjectURL 创建 @font-face，
 * 渲染示例文本，并提供字号（Slider）/ 字重（Select）/ 斜体（Switch）/ 自定义文本（Input）控制。
 * 视觉对齐 dufs PreviewDialog 的 FontPreview（shadcn 控件 + 语义色）。
 */
function FontPreview({ url, filename, onLoad }) {
  const { t } = useTranslation();
  const [fontSize, setFontSize] = useState(32);
  const [fontWeight, setFontWeight] = useState(400);
  const [fontItalic, setFontItalic] = useState(false);
  const [customText, setCustomText] = useState('');
  const [fontLoaded, setFontLoaded] = useState(false);
  const [error, setError] = useState(null);

  useEffect(() => {
    let cancelled = false;
    let objectUrl = null;
    let styleEl = null;
    const fontFamily = `preview-font-${filename.replace(/[^a-zA-Z0-9]/g, '-')}`;
    setFontLoaded(false);
    setError(null);

    fetch(url)
      .then((r) => {
        if (!r.ok) throw new Error('Failed to load font file');
        return r.blob();
      })
      .then((blob) => {
        if (cancelled) return;
        objectUrl = URL.createObjectURL(blob);
        styleEl = document.createElement('style');
        styleEl.textContent = [
          `@font-face {`,
          `  font-family: '${fontFamily}';`,
          `  src: url('${objectUrl}') format('${FONT_FORMATS[extname(filename)] || 'truetype'}');`,
          `  font-display: swap;`,
          `}`,
        ].join('\n');
        document.head.appendChild(styleEl);
        return document.fonts.load(`16px "${fontFamily}"`);
      })
      .then((faces) => {
        if (cancelled) return;
        // document.fonts.load 在失败时也会 resolve（face.status === 'error'）
        const ok = !faces || faces.some((f) => f.status === 'loaded');
        setFontLoaded(ok);
        onLoad?.();
      })
      .catch((err) => {
        if (!cancelled) {
          // 错误在组件内部展示（避免与 FilePreview 的通用错误 UI 重复）
          setError(err?.message || 'Failed to load font');
          onLoad?.();
        }
      });

    return () => {
      cancelled = true;
      if (styleEl) styleEl.remove();
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [url, filename]); // eslint-disable-line react-hooks/exhaustive-deps

  const fontFamily = `preview-font-${filename.replace(/[^a-zA-Z0-9]/g, '-')}`;
  const displayText = customText.trim() || t('fontPreviewText', { defaultValue: 'The quick brown fox jumps over the lazy dog.' });

  if (error) {
    return (
      <div className="text-center text-destructive py-8">
        <AlertTriangle size={36} className="mx-auto mb-2 opacity-60" />
        <p className="text-sm">{error}</p>
      </div>
    );
  }

  return (
    <div className="w-full py-4 space-y-6 max-h-[calc(100vh-200px)] overflow-auto">
      {/* 字体预览区域（对齐 dufs：bg-muted/30 + border + 圆角） */}
      <div
        className="text-center p-6 border border-border rounded-md bg-muted/30 min-h-[140px] flex items-center justify-center"
        style={{
          fontFamily: `'${fontFamily}', system-ui, -apple-system, sans-serif`,
          fontSize: `${fontSize}px`,
          fontWeight,
          fontStyle: fontItalic ? 'italic' : 'normal',
          opacity: fontLoaded ? 1 : 0.5,
        }}
      >
        <div className="break-all">
          <div>The quick brown fox jumps over the lazy dog.</div>
          <div>0123456789.:;&apos;&quot;(!?)+-*/=</div>
          <div>{displayText}</div>
        </div>
      </div>

      {/* 控制面板（shadcn Input / Slider / Select / Switch） */}
      <div className="space-y-4">
        <Input
          type="text"
          value={customText}
          onChange={(e) => setCustomText(e.target.value)}
          placeholder={t('fontPreviewTextPlaceholder', { defaultValue: '输入自定义预览文本...' })}
          className="w-full"
        />

        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-sm text-muted-foreground">
              {t('fontPreviewSize', { defaultValue: '字号' })}
            </span>
            <span className="text-sm font-medium tabular-nums">{fontSize}px</span>
          </div>
          <Slider
            value={[fontSize]}
            onValueChange={([v]) => setFontSize(Number(v))}
            min={12}
            max={72}
            step={1}
          />
        </div>

        <div className="flex items-center gap-4">
          <span className="text-sm text-muted-foreground whitespace-nowrap">
            {t('fontPreviewWeight', { defaultValue: '字重' })}
          </span>
          <Select value={fontWeight.toString()} onValueChange={(v) => setFontWeight(parseInt(v, 10))}>
            <SelectTrigger className="w-[150px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {FONT_WEIGHTS.map((w) => (
                <SelectItem key={w.value} value={w.value}>{w.label}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="flex items-center gap-2">
          <Switch checked={fontItalic} onCheckedChange={setFontItalic} />
          <span className="text-sm">{t('fontPreviewItalic', { defaultValue: '斜体' })}</span>
        </div>
      </div>
    </div>
  );
}

/**
 * 文本 / Markdown 预览：
 * - .md/.markdown → MarkdownRenderer（GFM + 代码高亮 + 行号）
 * - 代码文件 → Prism 高亮 + 行号（fm-markdown 作用域样式）
 * - 其余纯文本 → <pre>（保留原有行为）
 * 统一做 500KB 截断。
 */
function TextPreview({ url, filename, onLoad, onError }) {
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(true);
  const containerRef = useRef(null);

  const ext = extname(filename);
  const isMarkdown = MARKDOWN_EXTS.has(ext);
  const lang = CODE_LANG_BY_EXT[ext];
  const isCode = !isMarkdown && !!lang;

  useEffect(() => {
    let cancelled = false;
    fetch(url)
      .then((r) => {
        if (!r.ok) throw new Error('Failed');
        return r.text();
      })
      .then((text) => {
        if (!cancelled) {
          const maxLen = 500_000;
          setContent(text.length > maxLen ? text.slice(0, maxLen) + '\n\n[Content truncated...]' : text);
          setLoading(false);
          onLoad?.();
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setLoading(false);
          onError?.(err);
        }
      });
    return () => { cancelled = true; };
  }, [url]); // eslint-disable-line react-hooks/exhaustive-deps

  // 代码块在 DOM 提交后执行高亮 + 行号注入
  useEffect(() => {
    if (!loading && isCode && containerRef.current) {
      applyCodeHighlight(containerRef.current);
    }
  }, [loading, content, isCode]);

  // loading 时由外层 FilePreview 显示 spinner（骨架在图片/视频模式下由外层呈现）
  if (loading) return null;

  if (isMarkdown) {
    return (
      <div className="w-full max-h-[calc(100vh-220px)] overflow-auto">
        <MarkdownRenderer content={content} />
      </div>
    );
  }

  if (isCode) {
    // fm-markdown 作用域样式提供 Prism token 色 + 行号（.line-numbers-rows）
    return (
      <div
        ref={containerRef}
        className="fm-markdown w-full max-h-[calc(100vh-220px)] overflow-auto rounded-md"
        dangerouslySetInnerHTML={{ __html: buildCodeBlockHtml(content, lang) }}
      />
    );
  }

  return (
    <pre className="w-full max-h-[calc(100vh-220px)] overflow-auto text-xs font-mono leading-relaxed text-foreground bg-muted/40 border border-border rounded-md p-4 whitespace-pre-wrap break-all">
      {content}
    </pre>
  );
}

/**
 * 文件预览对话框（shadcn Dialog，视觉对齐 dufs PreviewDialog）。
 * Props: { entry, previewUrl, onClose, AudioPlayerComponent?, onNavigate? }
 * - 受控于宿主的条件渲染（宿主在 entry 存在时才挂载本组件），因此 Dialog 恒为打开态，
 *   Esc / 遮罩 / 关闭按钮统一走 onOpenChange(false) → onClose。
 * - onNavigate 可选：传入后头部显示"上一个/下一个"按钮并启用 ←/→ 键盘切换（调用 onNavigate(-1)/(1)）
 * - AudioPlayerComponent 可选：不传时音频显示"暂不支持预览"提示
 */
export function FilePreview({ entry, previewUrl, onClose, AudioPlayerComponent, onNavigate }) {
  const { t } = useTranslation();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  useEffect(() => {
    setLoading(true);
    setError(null);
  }, [entry?.name, previewUrl]);

  const hasNav = typeof onNavigate === 'function';

  // 键盘快捷键：←/→ 在同类型文件间切换（仅在提供 onNavigate 时）；Esc 由 Dialog 处理
  useEffect(() => {
    const handler = (e) => {
      const tag = e.target?.tagName;
      const isFormField = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || e.target?.isContentEditable;
      if (isFormField) return;
      if (e.key === 'ArrowLeft' && hasNav) onNavigate?.(-1);
      else if (e.key === 'ArrowRight' && hasNav) onNavigate?.(1);
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [hasNav, onNavigate]);

  if (!entry) return null;

  const mode = getPreviewMode(entry.name);
  const url = previewUrl || '#';
  const audioUnavailable = mode === 'audio' && !AudioPlayerComponent;
  // 无需异步加载的模式：不显示 spinner，直接渲染内容
  const noAsyncLoad = audioUnavailable || mode === 'unsupported';
  const showSpinner = loading && !noAsyncLoad;

  function handleLoad() { setLoading(false); }
  function handleError() { setError('Failed to load preview'); setLoading(false); }

  // 图片 / 视频加载骨架（dufs 风格：animate-pulse + bg-muted 占位）
  const mediaSkeleton = (
    <div className="w-full py-4 space-y-4 animate-pulse">
      <div className="w-full h-[calc(100vh-320px)] min-h-[220px] bg-muted rounded-md flex items-center justify-center">
        <File size={36} className="text-muted-foreground/40" />
      </div>
      <div className="h-3 bg-muted rounded w-2/3 mx-auto" />
    </div>
  );

  const genericSpinner = (
    <div className="flex items-center gap-2 text-muted-foreground">
      <Loader size={20} className="animate-spin" />
      <span>{t('loading')}</span>
    </div>
  );

  const errorBlock = (
    <div className="text-center text-destructive">
      <File size={40} className="mx-auto mb-2 opacity-50 text-muted-foreground/60" />
      <p className="text-sm">{error}</p>
    </div>
  );

  const content = (() => {
    switch (mode) {
      case 'image':
        return (
          <div className="w-full flex items-center justify-center py-4">
            <img
              src={url}
              alt={entry.name}
              onLoad={handleLoad}
              onError={handleError}
              className={cn('max-w-full max-h-[calc(100vh-260px)] object-contain rounded-md', (showSpinner || error) && 'hidden')}
            />
          </div>
        );
      case 'video':
        return (
          <div className="w-full flex items-center justify-center py-4">
            <video
              controls
              src={url}
              onLoadedData={handleLoad}
              onError={handleError}
              className={cn('max-w-full max-h-[calc(100vh-260px)] rounded-md', (showSpinner || error) && 'hidden')}
            />
          </div>
        );
      case 'audio':
        if (AudioPlayerComponent) {
          return (
            <div className="w-full">
              <AudioPlayerComponent
                src={url}
                filename={entry.name}
                onReady={handleLoad}
                onError={handleError}
              />
            </div>
          );
        }
        return (
          <div className="text-center text-muted-foreground py-8">
            <Music size={40} className="mx-auto mb-2 opacity-50" />
            <p className="text-sm">{t('audioPreviewUnavailable', { defaultValue: '该音频格式暂不支持预览，请下载后播放' })}</p>
          </div>
        );
      case 'pdf':
        return (
          <iframe
            src={url}
            type="application/pdf"
            title={entry.name}
            onLoad={handleLoad}
            onError={handleError}
            className={cn('w-full rounded-md border-0', (showSpinner || error) && 'hidden')}
            style={{ height: 'calc(100vh - 240px)' }}
          />
        );
      case 'font':
        return (
          <div className={cn('w-full', showSpinner && 'hidden')}>
            <FontPreview url={url} filename={entry.name} onLoad={handleLoad} />
          </div>
        );
      case 'office':
        return (
          <div className={cn('w-full', showSpinner && 'hidden')}>
            <OfficePreview
              url={url}
              filename={entry.name}
              size={entry.size}
              onLoad={handleLoad}
            />
          </div>
        );
      case 'markdown':
      case 'text':
        return (
          <div className={cn('w-full', showSpinner && 'hidden')}>
            <TextPreview
              url={url}
              filename={entry.name}
              onLoad={handleLoad}
              onError={handleError}
            />
          </div>
        );
      default:
        return (
          <div className="text-center text-muted-foreground py-8">
            <File size={40} className="mx-auto mb-2 opacity-50" />
            <p className="text-sm">{t('previewUnavailable', { defaultValue: '该文件类型暂不支持预览' })}</p>
            <a
              href={url}
              download={entry.name}
              className="inline-block mt-3 text-primary hover:underline text-sm"
            >
              {t('download')}
            </a>
          </div>
        );
    }
  })();

  return (
    <Dialog open onOpenChange={(v) => { if (!v) onClose?.(); }}>
      <DialogContent
        showCloseButton={false}
        className="max-w-6xl sm:max-w-6xl max-h-[92vh] flex flex-col p-0"
      >
        {/* 头部（对齐 dufs PreviewDialog：文件名 + 大小 + 导航/下载/关闭） */}
        <DialogHeader className="px-4 py-3 border-b flex flex-row items-center gap-2 shrink-0">
          <DialogTitle className="flex-1 min-w-0 text-base font-medium truncate text-left">
            {entry.name}
            {!entry.is_dir && entry.size != null && (
              <span className="hidden sm:inline text-muted-foreground ml-2 font-normal">
                ({formatSize(entry.size)})
              </span>
            )}
          </DialogTitle>
          <DialogDescription className="sr-only">
            {entry.name} preview
          </DialogDescription>

          {/* 上一个 / 下一个（仅在有 onNavigate 时显示） */}
          {hasNav && (
            <>
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8"
                onClick={() => onNavigate(-1)}
                title={t('prevFile', { defaultValue: '上一个' })}
              >
                <ChevronLeft size={16} />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="h-8 w-8"
                onClick={() => onNavigate(1)}
                title={t('nextFile', { defaultValue: '下一个' })}
              >
                <ChevronRight size={16} />
              </Button>
            </>
          )}

          {/* 下载 */}
          {!entry.is_dir && (
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8"
              asChild
              title={t('download')}
            >
              <a href={url} download={entry.name}>
                <Download size={16} />
              </a>
            </Button>
          )}

          {/* 关闭 */}
          <Button
            variant="ghost"
            size="icon"
            className="h-8 w-8"
            onClick={onClose}
            title={t('close', { defaultValue: '关闭' })}
          >
            <X size={16} />
          </Button>
        </DialogHeader>

        {/* 内容区域：flex-1 滚动，媒体类居中显示；loading 时按模式显示骨架或 spinner */}
        <div className="flex-1 min-h-[200px] overflow-auto px-4 flex items-center justify-center">
          {error ? (
            errorBlock
          ) : (
            <>
              {showSpinner && ((mode === 'image' || mode === 'video') ? mediaSkeleton : genericSpinner)}
              {content}
            </>
          )}
        </div>

        {/* 底部：类型标识 + 下载 */}
        <div className="shrink-0 flex items-center justify-between px-4 py-2 border-t border-border text-xs text-muted-foreground">
          <span>{entry.is_dir ? 'directory' : (mode === 'office' || mode === 'text' || mode === 'markdown') ? extname(entry.name) || mode : mode}</span>
          {!entry.is_dir && (
            <a href={url} download={entry.name} className="text-primary hover:underline">
              {t('download')}
            </a>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}