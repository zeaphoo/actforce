import React, { useState } from 'react';
import {
  Folder, File, Image, Video, Music, Archive, FileText, FileCode, Disc, Container,
  GitBranch, Terminal, Database, BookOpen, Type, Magnet, AppWindow, Smartphone,
  ChevronLeft, ChevronRight, ArrowUpDown, ArrowUp, ArrowDown, CornerDownLeft, Link2,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { formatSize, formatDate, cn, extname } from '../lib/utils.js';
import {
  Table, TableHeader, TableBody, TableHead, TableRow, TableCell,
} from './ui/table.jsx';
import { Button } from './ui/button.jsx';
import { Input } from './ui/input.jsx';
import { FileActions } from './FileActions.jsx';

// Clicking a column cycles: asc → desc → back to the default sort (name asc)
const DEFAULT_SORT = { field: 'name', order: 'asc' };

// --- Static extension → [lucide icon, tailwind color class] mapping (~110 exts) ---
// Pure static map (no dufs logic); colors are 400–700 level so they read in both themes.
const EXT_ICON_MAP = {
  // Audio
  mp3: [Music, 'text-purple-500'], flac: [Music, 'text-purple-500'],
  ape: [Music, 'text-purple-500'], wav: [Music, 'text-purple-500'],
  ogg: [Music, 'text-purple-500'], m4a: [Music, 'text-purple-500'],
  opus: [Music, 'text-purple-500'], oga: [Music, 'text-purple-500'],
  weba: [Music, 'text-purple-500'],
  // Video
  mp4: [Video, 'text-red-500'], flv: [Video, 'text-red-500'],
  wmv: [Video, 'text-red-500'], rm: [Video, 'text-red-500'],
  rmvb: [Video, 'text-red-500'], mkv: [Video, 'text-red-500'],
  webm: [Video, 'text-red-500'], avi: [Video, 'text-red-500'],
  m3u8: [Video, 'text-red-500'], mov: [Video, 'text-red-500'],
  ogv: [Video, 'text-red-500'],
  // Image
  bmp: [Image, 'text-sky-500'], png: [Image, 'text-sky-500'],
  gif: [Image, 'text-sky-500'], jpg: [Image, 'text-sky-500'],
  jpeg: [Image, 'text-sky-500'], psd: [Image, 'text-sky-500'],
  ai: [Image, 'text-sky-500'], webp: [Image, 'text-sky-500'],
  avif: [Image, 'text-sky-500'], tiff: [Image, 'text-sky-500'],
  heif: [Image, 'text-sky-500'], heic: [Image, 'text-sky-500'],
  jxl: [Image, 'text-sky-500'], svg: [FileCode, 'text-orange-500'],
  // Text / config
  txt: [FileText, 'text-slate-500'], md: [FileText, 'text-slate-500'],
  markdown: [FileText, 'text-slate-500'], ini: [FileText, 'text-slate-500'],
  conf: [FileText, 'text-slate-500'], yml: [FileText, 'text-slate-500'],
  yaml: [FileText, 'text-slate-500'], toml: [FileText, 'text-slate-500'],
  log: [FileText, 'text-slate-500'], sh: [Terminal, 'text-slate-500'],
  cmd: [Terminal, 'text-slate-500'], bat: [Terminal, 'text-slate-500'],
  csv: [FileText, 'text-slate-500'], tsv: [FileText, 'text-slate-500'],
  rtf: [FileText, 'text-slate-500'],
  // Archives
  zip: [Archive, 'text-amber-500'], tar: [Archive, 'text-amber-500'],
  gz: [Archive, 'text-amber-500'], bz2: [Archive, 'text-amber-500'],
  xz: [Archive, 'text-amber-500'], zst: [Archive, 'text-amber-500'],
  rar: [Archive, 'text-amber-500'], '7z': [Archive, 'text-amber-500'],
  // Disc images
  iso: [Disc, 'text-teal-500'], mdf: [Disc, 'text-teal-500'],
  mds: [Disc, 'text-teal-500'],
  // Code
  c: [FileCode, 'text-blue-600'], h: [FileCode, 'text-blue-600'],
  cc: [FileCode, 'text-blue-700'], cpp: [FileCode, 'text-blue-700'],
  cxx: [FileCode, 'text-blue-700'], hpp: [FileCode, 'text-blue-700'],
  cs: [FileCode, 'text-green-600'], go: [FileCode, 'text-cyan-500'],
  java: [FileCode, 'text-red-600'], js: [FileCode, 'text-yellow-500'],
  jsx: [FileCode, 'text-yellow-500'], cjs: [FileCode, 'text-yellow-500'],
  mjs: [FileCode, 'text-yellow-500'], ts: [FileCode, 'text-blue-500'],
  tsx: [FileCode, 'text-blue-500'], php: [FileCode, 'text-indigo-500'],
  py: [FileCode, 'text-green-600'], pyi: [FileCode, 'text-green-600'],
  pyc: [FileCode, 'text-green-600'], pyx: [FileCode, 'text-green-600'],
  html: [FileCode, 'text-orange-500'], htm: [FileCode, 'text-orange-500'],
  css: [FileCode, 'text-blue-500'], scss: [FileCode, 'text-pink-500'],
  lua: [FileCode, 'text-indigo-700'], rs: [FileCode, 'text-orange-700'],
  kt: [FileCode, 'text-purple-500'], ps1: [FileCode, 'text-blue-600'],
  json: [FileCode, 'text-yellow-600'], xml: [FileCode, 'text-orange-600'],
  vue: [FileCode, 'text-green-500'], ipynb: [FileCode, 'text-orange-600'],
  tex: [FileText, 'text-slate-500'],
  // Databases
  db: [Database, 'text-cyan-700'], sql: [Database, 'text-cyan-700'],
  sqlite: [Database, 'text-cyan-700'],
  // Office documents
  doc: [FileText, 'text-blue-500'], docx: [FileText, 'text-blue-500'],
  xls: [FileText, 'text-green-600'], xlsx: [FileText, 'text-green-600'],
  ppt: [FileText, 'text-orange-600'], pptx: [FileText, 'text-orange-600'],
  pdf: [FileText, 'text-red-500'], epub: [BookOpen, 'text-lime-600'],
  // Fonts
  ttf: [Type, 'text-pink-500'], ttc: [Type, 'text-pink-500'],
  otf: [Type, 'text-pink-500'], otc: [Type, 'text-pink-500'],
  woff: [Type, 'text-pink-500'], woff2: [Type, 'text-pink-500'],
  // Misc
  torrent: [Magnet, 'text-indigo-500'], exe: [AppWindow, 'text-indigo-500'],
  msi: [AppWindow, 'text-indigo-500'], apk: [Smartphone, 'text-green-600'],
};

// Extension-less / special filenames
const NAME_ICON_MAP = {
  dockerfile: [Container, 'text-blue-600'],
  makefile: [FileCode, 'text-slate-500'],
  gitignore: [GitBranch, 'text-red-600'],
  license: [FileText, 'text-slate-500'],
  readme: [FileText, 'text-slate-500'],
  env: [FileText, 'text-slate-500'],
};

function EntryIcon({ entry }) {
  const size = 20;
  if (entry.is_dir) return <Folder size={size} className="text-yellow-500" />;
  const name = entry.name || '';
  const ext = extname(name);
  if (ext) {
    const hit = EXT_ICON_MAP[ext];
    if (hit) {
      const [Icon, cls] = hit;
      return <Icon size={size} className={cls} />;
    }
  }
  const base = name.toLowerCase().replace(/^\.+/, '');
  const nameHit = NAME_ICON_MAP[base];
  if (nameHit) {
    const [Icon, cls] = nameHit;
    return <Icon size={size} className={cls} />;
  }
  return <File size={size} className="text-muted-foreground" />;
}

function SortHeader({ label, field, current, order, onClick, align = 'left' }) {
  const { t } = useTranslation();
  const isActive = current === field;
  return (
    <button
      onClick={() => onClick(field)}
      title={isActive ? (order === 'asc' ? t('sortAscending') : t('sortDescending')) : undefined}
      className={cn(
        'flex items-center gap-1 hover:text-primary transition-colors',
        align === 'right' && 'justify-end w-full'
      )}
    >
      <span className="truncate">{label}</span>
      {isActive ? (
        order === 'asc'
          ? <ArrowUp size={12} className="shrink-0 text-primary" />
          : <ArrowDown size={12} className="shrink-0 text-primary" />
      ) : (
        <ArrowUpDown size={12} className="shrink-0 text-muted-foreground" />
      )}
    </button>
  );
}

// Page numbers centered on the current page (±2), with first/last + ellipsis
function getPageWindow(current, total) {
  const pages = [];
  let start = Math.max(1, current - 2);
  let end = Math.min(total, current + 2);
  if (end - start + 1 < 5) {
    if (start === 1) end = Math.min(total, start + 4);
    else start = Math.max(1, end - 4);
  }
  if (start > 1) {
    pages.push(1);
    if (start > 2) pages.push('…');
  }
  for (let i = start; i <= end; i++) pages.push(i);
  if (end < total) {
    if (end < total - 1) pages.push('…');
    pages.push(total);
  }
  return pages;
}

function SkeletonRows() {
  return Array.from({ length: 5 }, (_, i) => (
    <TableRow key={i}>
      <TableCell>
        <div className="flex items-center gap-2">
          <div className="size-5 rounded bg-muted animate-pulse shrink-0" />
          <div className="h-4 w-32 max-w-[240px] rounded bg-muted animate-pulse" />
        </div>
      </TableCell>
      <TableCell className="text-right whitespace-nowrap hidden sm:table-cell">
        <div className="h-4 w-14 ml-auto rounded bg-muted animate-pulse" />
      </TableCell>
      <TableCell className="text-right whitespace-nowrap hidden md:table-cell">
        <div className="h-4 w-20 ml-auto rounded bg-muted animate-pulse" />
      </TableCell>
      <TableCell className="text-right">
        <div className="flex justify-end">
          <div className="size-8 rounded bg-muted animate-pulse" />
        </div>
      </TableCell>
    </TableRow>
  ));
}

export function FileBrowser({
  entries, total, loading, error,
  currentPath, sort, order, page,
  totalPages,
  onNavigate, onSort, onPageChange,
  onPreview, onDownload, onDelete, onRename, onCopyLink, onZip,
  onDoubleClick,
  renderActions,
}) {
  const { t } = useTranslation();
  const [selected, setSelected] = useState(null);
  const [jumpPage, setJumpPage] = useState('');

  // Sort cycle: asc → desc → default (name asc)
  function handleSort(field) {
    if (sort !== field) {
      onSort?.(field, 'asc');
      return;
    }
    if (order === 'asc') {
      onSort?.(field, 'desc');
      return;
    }
    onSort?.(DEFAULT_SORT.field, DEFAULT_SORT.order);
  }

  function handleRowClick(entry, e) {
    if (e.target.closest('button') || e.target.closest('a') || e.target.closest('input')) return;
    if (entry.is_dir) {
      onNavigate?.(currentPath.replace(/\/+$/, '') + '/' + entry.name);
    } else {
      setSelected(selected === entry.name ? null : entry.name);
      onPreview?.(entry);
    }
  }

  function handleDblClick(entry) {
    if (onDoubleClick) {
      onDoubleClick(entry);
      return;
    }
    if (entry.is_dir) {
      onNavigate?.(currentPath.replace(/\/+$/, '') + '/' + entry.name);
    } else {
      onDownload?.(entry);
    }
  }

  function handleJumpSubmit(e) {
    e.preventDefault();
    const n = parseInt(jumpPage, 10);
    if (!Number.isNaN(n) && n >= 1 && n <= totalPages) {
      onPageChange?.(n);
    }
    setJumpPage('');
  }

  // Directory size column: child count from the backend's children_count field
  function dirSizeLabel(entry) {
    if (entry.children_count == null) return '--';
    return entry.children_count > 999
      ? t('subdirItemsMany')
      : t('subdirItems', { count: entry.children_count });
  }

  const sortLabel = (field) => {
    const map = { name: t('sortByName'), modified: t('sortByModified'), size: t('sortBySize') };
    return map[field] || field;
  };

  return (
    <div className="w-full space-y-4">
      {/* File list table — shadcn Table, visual aligned with dufs FileList */}
      <div className={cn(
        'rounded-md border overflow-x-auto transition-opacity',
        loading && entries.length > 0 && 'opacity-50 pointer-events-none'
      )}>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-full">
                <SortHeader label={sortLabel('name')} field="name" current={sort} order={order} onClick={handleSort} />
              </TableHead>
              <TableHead className="text-right whitespace-nowrap hidden sm:table-cell">
                <SortHeader label={sortLabel('size')} field="size" current={sort} order={order} onClick={handleSort} align="right" />
              </TableHead>
              <TableHead className="text-right whitespace-nowrap hidden md:table-cell">
                <SortHeader label={sortLabel('modified')} field="modified" current={sort} order={order} onClick={handleSort} align="right" />
              </TableHead>
              <TableHead className="text-right whitespace-nowrap">{t('actions')}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {loading && entries.length === 0 && <SkeletonRows />}

            {error && entries.length === 0 && (
              <TableRow>
                <TableCell colSpan={4} className="py-8 text-center text-destructive">
                  {error}
                </TableCell>
              </TableRow>
            )}

            {!loading && !error && entries.length === 0 && (
              <TableRow>
                <TableCell colSpan={4} className="py-8 text-center text-muted-foreground">
                  <Folder size={32} className="mx-auto mb-2 opacity-30" />
                  <p>{t('emptyFolder')}</p>
                </TableCell>
              </TableRow>
            )}

            {entries.map((entry) => (
              <TableRow
                key={entry.name}
                className="cursor-pointer"
                data-state={selected === entry.name ? 'selected' : undefined}
                onClick={(e) => handleRowClick(entry, e)}
                onDoubleClick={() => handleDblClick(entry)}
              >
                <TableCell>
                  <div className="flex items-center gap-2 min-w-[200px]">
                    <span className="shrink-0">
                      <EntryIcon entry={entry} />
                    </span>
                    <span className={cn('break-all', entry.is_dir && 'font-medium')}>
                      {entry.name}
                    </span>
                    {entry.is_symlink && (
                      <Link2
                        size={13}
                        className="shrink-0 text-muted-foreground"
                        title={t('symlink')}
                        aria-label={t('symlink')}
                      />
                    )}
                  </div>
                </TableCell>
                <TableCell className="text-right whitespace-nowrap hidden sm:table-cell tabular-nums">
                  {entry.is_dir ? dirSizeLabel(entry) : formatSize(entry.size)}
                </TableCell>
                <TableCell className="text-right whitespace-nowrap hidden md:table-cell tabular-nums">
                  {formatDate(entry.modified || entry.mtime)}
                </TableCell>
                <TableCell className="text-right">
                  {renderActions ? renderActions(entry) : (
                    <div className="flex items-center justify-end">
                      <FileActions
                        entry={entry}
                        onDownload={onDownload}
                        onDelete={onDelete}
                        onRename={onRename}
                        onCopyLink={onCopyLink}
                        onZip={onZip}
                      />
                    </div>
                  )}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-between gap-3 px-1 text-xs text-muted-foreground">
          <span className="tabular-nums shrink-0">{total} {t('items')}</span>
          <div className="flex items-center gap-1">
            <Button
              variant="outline"
              size="icon-sm"
              onClick={() => onPageChange?.(page - 1)}
              disabled={page <= 1}
              aria-label={t('page')}
              title={t('page')}
            >
              <ChevronLeft size={14} />
            </Button>

            {getPageWindow(page, totalPages).map((p, i) => (
              p === '…' ? (
                <span key={`ellipsis-${i}`} className="px-0.5 text-muted-foreground">…</span>
              ) : (
                <Button
                  key={p}
                  variant={p === page ? 'default' : 'outline'}
                  size="icon-sm"
                  className="h-8 w-8 p-0 text-xs"
                  onClick={() => onPageChange?.(p)}
                >
                  {p}
                </Button>
              )
            ))}

            <Button
              variant="outline"
              size="icon-sm"
              onClick={() => onPageChange?.(page + 1)}
              disabled={page >= totalPages}
              aria-label={t('page')}
              title={t('page')}
            >
              <ChevronRight size={14} />
            </Button>

            <form onSubmit={handleJumpSubmit} className="flex items-center gap-1 ml-2">
              <Input
                type="number"
                min={1}
                max={totalPages}
                value={jumpPage}
                onChange={(e) => setJumpPage(e.target.value)}
                placeholder={String(page)}
                aria-label={t('goToPage')}
                title={t('goToPage')}
                className="w-14 h-8 px-1 text-center text-xs"
              />
              <Button
                type="submit"
                variant="ghost"
                size="icon-sm"
                className="h-8 w-8 text-muted-foreground"
                title={t('goToPage')}
                aria-label={t('goToPage')}
              >
                <CornerDownLeft size={12} />
              </Button>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}