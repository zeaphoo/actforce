import React, { useState, useEffect, useRef } from 'react';
import { FileText, ChevronRight, Loader } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { cn } from '../lib/utils.js';
import { MarkdownRenderer } from './MarkdownRenderer.jsx';
import { Card, CardContent } from './ui/card.jsx';

/**
 * README 卡片（shadcn Card 包装）。
 * - 传 `url` 时：直接抓取该地址的内容（配合 `headers` 传认证头）。
 * - 不传 `url` 时：按 `basePath` + `currentPath` 探测常见 README 文件名（向后兼容）。
 */
export function ReadmeCard({ url, headers, basePath, currentPath }) {
  const { t } = useTranslation();
  const [content, setContent] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [expanded, setExpanded] = useState(true);
  const [isMarkdown, setIsMarkdown] = useState(false);

  // headers 的引用可能每次渲染都变化，用 ref 保持稳定，避免 effect 反复触发
  const headersRef = useRef(headers);
  useEffect(() => { headersRef.current = headers; }, [headers]);

  useEffect(() => {
    let cancelled = false;
    setContent(null);
    setError(null);

    async function fetchText(fetchUrl) {
      const res = await fetch(fetchUrl, { headers: headersRef.current });
      if (!res.ok) return null;
      return res.text();
    }

    async function loadFromUrl() {
      try {
        setLoading(true);
        const text = await fetchText(url);
        if (cancelled) return;
        if (text != null) {
          setContent(text.slice(0, 100000));
          setIsMarkdown(/\.(md|markdown)$/i.test(url));
        }
      } catch { /* ignore */ } finally {
        if (!cancelled) setLoading(false);
      }
    }

    async function probeReadme() {
      const dir = currentPath.replace(/\/+$/, '') || '';
      const candidates = [
        { url: `${basePath}${dir}/README.md`, isMd: true },
        { url: `${basePath}${dir}/readme.md`, isMd: true },
        { url: `${basePath}${dir}/README.txt`, isMd: false },
        { url: `${basePath}${dir}/readme.txt`, isMd: false },
        { url: `${basePath}${dir}/README`, isMd: false },
        { url: `${basePath}${dir}/readme`, isMd: false },
      ];

      for (const { url: candidate, isMd } of candidates) {
        if (cancelled) return;
        try {
          setLoading(true);
          const text = await fetchText(candidate);
          if (text != null) {
            if (!cancelled) {
              setContent(text.slice(0, 100000));
              setIsMarkdown(isMd || candidate.endsWith('.md'));
              setLoading(false);
            }
            return;
          }
        } catch { /* try next */ }
      }
      if (!cancelled) setLoading(false);
    }

    if (url) {
      loadFromUrl();
    } else {
      probeReadme();
    }
    return () => { cancelled = true; };
  }, [url, basePath, currentPath]);

  if (loading) {
    return (
      <Card className="gap-0 py-2 shadow-none rounded-lg">
        <CardContent className="px-3 py-2 text-xs text-muted-foreground">
          <Loader size={14} className="animate-spin inline mr-1" />
          {t('checkingReadme')}
        </CardContent>
      </Card>
    );
  }

  if (error) {
    return (
      <Card className={cn('gap-0 py-2 shadow-none rounded-lg', 'border-destructive/50')}>
        <CardContent className="px-3 py-2 text-xs text-destructive">
          {error}
        </CardContent>
      </Card>
    );
  }

  if (!content) return null;

  return (
    <Card className="gap-0 py-0 shadow-none rounded-lg overflow-hidden">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center justify-between w-full px-4 py-2 text-xs font-medium text-muted-foreground hover:bg-muted/50 transition-colors"
      >
        <span className="flex items-center gap-1.5">
          <FileText size={14} className="shrink-0" />
          README
        </span>
        <ChevronRight size={12} className={cn('shrink-0 transition-transform', expanded ? 'rotate-90' : '')} />
      </button>
      {expanded && (
        <div className="px-4 py-3 border-t max-h-[500px] overflow-y-auto text-xs">
          {isMarkdown ? (
            <MarkdownRenderer content={content} />
          ) : (
            <pre className="font-mono text-muted-foreground whitespace-pre-wrap">
              {content}
            </pre>
          )}
        </div>
      )}
    </Card>
  );
}