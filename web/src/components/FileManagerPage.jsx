import React, { useState, useCallback, useMemo } from 'react';
import {
  FolderOpen, Upload, FolderDown, FolderPlus, FilePlus, RefreshCw, ArrowLeft,
  Search, X, Home, ChevronRight,
} from 'lucide-react';
import {
  FileBrowser, useFileService, FileUploader,
  FilePreview, CreateDialog, EditDialog, ReadmeCard,
  Button, Tooltip, TooltipTrigger, TooltipContent, TooltipProvider,
  Card, CardContent, Input,
} from '@actforce/file-manager';
import { getAuthHeaders } from '../lib/auth';

// Extensions that can be edited inline (mirrors dufs-astro-assets text preview set)
const TEXT_EXTS = new Set([
  'txt', 'md', 'markdown', 'json', 'csv', 'py', 'js', 'jsx', 'ts', 'tsx',
  'html', 'htm', 'css', 'scss', 'xml', 'yaml', 'yml', 'log', 'sql', 'sh',
  'ini', 'toml', 'conf', 'env', 'gitignore', 'svg',
]);

/**
 * 全屏文件管理页面（替代原来的 FileBrowserDialog 弹窗）。
 *
 * 操作结构对齐 dufs-astro-assets FileListPage（FileListPage.jsx + AppBar.jsx）：
 * - 顶栏（AppBar 风格，深色 secondary 底）集成了全部操作：
 *   左侧标题；中部 = 刷新按钮 + 面包屑（overflow-x-auto 横向滚动，Button link 白字 + ChevronRight）
 *   + 深色搜索框；右侧 = 上传文件 / 上传文件夹 | 新建文件夹 / 新建文件；再分隔线 + 返回聊天。
 *   按钮均为 variant="ghost" size="icon" + Tooltip（对齐 dufs 501-603 行的写法）。
 * - 上传面板：FileUploader 常驻挂载（关闭时 hidden），顶栏“上传文件”切换显示并打开文件选择器，
 *   “上传文件夹”打开面板并直接打开文件夹选择器（webkitdirectory）。
 * - 列表区：Card > CardContent p-4 包裹 FileBrowser（对齐 dufs 655-680），README 卡片在其上方。
 * - 底部 Footer：当前路径 + 条目数。
 *
 * 交互差异说明：dufs 行单击即预览（FileList.jsx handleRowClick → onFileClick），编辑走下拉菜单；
 * 本页保留单机预览（已对齐 dufs），并额外保留“双击文本文件内联编辑 / 双击目录导航”的增强
 * （FileBrowser onDoubleClick），不冲突、不破坏现有功能。
 */
export function FileManagerPage({ onBack }) {
  const service = useFileService({ basePath: '/api/disks/default' });
  const [showUploader, setShowUploader] = useState(false);
  const [fileRequest, setFileRequest] = useState(0);
  const [folderRequest, setFolderRequest] = useState(0);
  const [showCreate, setShowCreate] = useState(false);
  const [createMode, setCreateMode] = useState('file');
  const [previewEntry, setPreviewEntry] = useState(null);
  const [editEntry, setEditEntry] = useState(null);
  const [editUrl, setEditUrl] = useState('');
  const [editContent, setEditContent] = useState('');
  const [renameEntry, setRenameEntry] = useState(null);
  const [renameValue, setRenameValue] = useState('');

  // Path of an entry relative to basePath (/disks/default)
  const entryPath = useCallback((entry) => {
    const dir = service.currentPath.replace(/\/+$/, '');
    return `${dir}/${entry.name}`;
  }, [service.currentPath]);

  const isTextFile = useCallback((entry) => {
    if (entry.is_dir) return false;
    const ext = (entry.name.split('.').pop() || '').toLowerCase();
    return TEXT_EXTS.has(ext);
  }, []);

  // README entry in the current directory listing (readme.md / README.txt / readme.markdown / readme)
  const readmeEntry = service.entries.find(
    (e) => !e.is_dir && /^readme(\.md|\.txt|\.markdown)?$/i.test(e.name)
  );
  const isSearching = service.searchResults != null;

  // 面包屑路径项（对齐 dufs FileListPage 108-118：root 图标 + 各段 Button link + ChevronRight）
  const breadcrumbItems = useMemo(() => {
    const segments = service.currentPath.split('/').filter(Boolean);
    return segments.map((segment, i) => ({
      title: segment,
      href: '/' + segments.slice(0, i + 1).join('/'),
    }));
  }, [service.currentPath]);

  const handleRefresh = useCallback(() => {
    service.listDir(service.currentPath);
  }, [service]);

  const handlePreview = useCallback((entry) => {
    if (entry && !entry.is_dir) {
      setPreviewEntry(entry);
    }
  }, []);

  // prev/next navigation across files in the current directory (FilePreview onNavigate)
  const handlePreviewNavigate = useCallback((delta) => {
    setPreviewEntry((prev) => {
      if (!prev) return prev;
      const files = service.entries.filter((e) => !e.is_dir);
      if (files.length < 2) return prev;
      const idx = files.findIndex((e) => e.name === prev.name);
      if (idx === -1) return prev;
      return files[(idx + delta + files.length) % files.length];
    });
  }, [service.entries]);

  // Double-click: folder → navigate; text file → inline editor; else → preview
  const handleDoubleClick = useCallback(async (entry) => {
    if (entry.is_dir) {
      service.navigateTo(entryPath(entry));
      return;
    }
    if (isTextFile(entry)) {
      const url = service.getPreviewUrl(entryPath(entry));
      try {
        const res = await fetch(url, { headers: getAuthHeaders() });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const text = await res.text();
        setEditUrl(url);
        setEditContent(text);
        setEditEntry(entry);
      } catch (e) {
        console.error('Failed to load file for editing:', e);
      }
      return;
    }
    handlePreview(entry);
  }, [service, entryPath, isTextFile, handlePreview]);

  const handleRename = useCallback((entry) => {
    setRenameEntry(entry);
    setRenameValue(entry.name);
  }, []);

  const confirmRename = useCallback(async () => {
    const target = renameEntry;
    const newName = (renameValue || '').trim();
    setRenameEntry(null);
    if (!target || !newName || newName === target.name) return;
    const dir = service.currentPath.replace(/\/+$/, '');
    try {
      await service.moveEntry(`${dir}/${target.name}`, `${dir}/${newName}`);
      service.listDir(service.currentPath);
    } catch (e) {
      console.error('Rename failed:', e);
    }
  }, [renameEntry, renameValue, service]);

  const handleDelete = useCallback(async (entry) => {
    const label = entry.is_dir ? '文件夹' : '文件';
    const ok = window.confirm(`确定要删除${label} "${entry.name}" 吗？`);
    if (!ok) return;
    try {
      await service.deleteEntry(entryPath(entry));
    } catch (e) {
      console.error('Delete failed:', e);
    }
  }, [service, entryPath]);

  const handleCopyLink = useCallback(async (entry) => {
    const url = service.getPreviewUrl(entryPath(entry));
    try {
      await navigator.clipboard.writeText(window.location.origin + url);
    } catch (e) {
      console.error('Copy failed:', e);
    }
  }, [service, entryPath]);

  // 顶栏“上传文件”：切换显示上传面板；面板关闭时顺带请求打开文件选择器
  const handleUploadFileClick = useCallback(() => {
    if (!showUploader) {
      setFileRequest((n) => n + 1);
    }
    setShowUploader((v) => !v);
  }, [showUploader]);

  // 顶栏“上传文件夹”：打开上传面板并请求打开文件夹选择器（webkitdirectory，整个目录）
  const handleUploadFolderClick = useCallback(() => {
    setShowUploader(true);
    setFolderRequest((n) => n + 1);
  }, []);

  const handleCreateFolder = useCallback(() => {
    setCreateMode('dir');
    setShowCreate(true);
  }, []);

  const handleCreateFile = useCallback(() => {
    setCreateMode('file');
    setShowCreate(true);
  }, []);

  const handleSearchChange = useCallback((value) => {
    service.setSearchQuery(value);
    service.debouncedSearch(value);
  }, [service]);

  const handleSearchClear = useCallback(() => {
    service.setSearchQuery('');
    service.debouncedSearch('');
  }, [service]);

  return (
    <TooltipProvider delayDuration={300}>
      <div className="h-screen w-full flex flex-col bg-background text-foreground">
        {/* ===== 顶栏（AppBar 风格，参考 dufs AppBar.jsx + FileListPage portal 区）===== */}
        <header
          className="shrink-0 h-12 flex items-center gap-2 px-4 border-b bg-secondary text-secondary-foreground relative z-40"
          style={{ boxShadow: '0 2px 8px 0px hsl(0 0% 10.1961% / 0.15)' }}
        >
          {/* 左侧：图标 + 标题 */}
          <div className="flex items-center gap-2 shrink-0">
            <FolderOpen size={18} />
            <h1 className="text-base font-semibold whitespace-nowrap">文件管理</h1>
          </div>

          {/* 中部：刷新 + 面包屑 + 搜索（对齐 dufs 面包屑 portal） */}
          <div className="flex items-center gap-1 flex-1 min-w-0">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8 shrink-0 text-primary-foreground hover:bg-primary-foreground/10"
                  onClick={handleRefresh}
                >
                  <RefreshCw className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>刷新</TooltipContent>
            </Tooltip>

            {/* 回到根目录 */}
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-8 w-8 shrink-0 text-primary-foreground hover:bg-primary-foreground/10"
                  onClick={() => service.navigateTo('/')}
                >
                  <Home className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>回到根目录</TooltipContent>
            </Tooltip>

            {/* 面包屑路径 - 支持横向滚动（对齐 dufs 608-624） */}
            <nav className="flex items-center overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden flex-1 min-w-0">
              {breadcrumbItems.map((item, index) => (
                <div key={item.href} className="flex items-center shrink-0">
                  <Button
                    variant="link"
                    className="h-8 px-1 text-sm whitespace-nowrap text-primary-foreground/80 hover:text-primary-foreground"
                    onClick={() => service.navigateTo(item.href)}
                  >
                    {item.title}
                  </Button>
                  {index < breadcrumbItems.length - 1 && (
                    <ChevronRight className="h-4 w-4 text-primary-foreground/50 shrink-0" />
                  )}
                </div>
              ))}
            </nav>

            {/* 搜索框（对齐 dufs 627-648：深色 Input + Search 图标 + X 清除） */}
            <div className="relative shrink-0 ml-2 w-40 sm:w-56 md:w-72">
              <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-primary-foreground/50" />
              <Input
                type="text"
                placeholder="搜索文件…"
                value={service.searchQuery}
                onChange={(e) => handleSearchChange(e.target.value)}
                className="pl-9 pr-9 h-8 bg-primary-foreground/10 border-primary-foreground/20 text-primary-foreground placeholder:text-primary-foreground/40"
              />
              {service.searchQuery && (
                <Button
                  variant="ghost"
                  size="icon"
                  className="absolute right-0 top-0 h-8 w-8 text-primary-foreground hover:bg-primary-foreground/10"
                  onClick={handleSearchClear}
                  aria-label="清除搜索"
                >
                  <X className="h-4 w-4" />
                </Button>
              )}
            </div>
          </div>

          {/* 右侧操作区：上传文件 / 上传文件夹 | 新建文件夹 / 新建文件（对齐 dufs 操作 portal） */}
          <div className="flex items-center gap-1 shrink-0">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-9 w-9 text-primary-foreground hover:bg-primary-foreground/10"
                  onClick={handleUploadFileClick}
                >
                  <Upload className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>上传文件</TooltipContent>
            </Tooltip>

            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-9 w-9 text-primary-foreground hover:bg-primary-foreground/10"
                  onClick={handleUploadFolderClick}
                >
                  <FolderDown className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>上传文件夹</TooltipContent>
            </Tooltip>

            <div className="w-px h-6 bg-primary-foreground/20" />

            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-9 w-9 text-primary-foreground hover:bg-primary-foreground/10"
                  onClick={handleCreateFolder}
                >
                  <FolderPlus className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>新建文件夹</TooltipContent>
            </Tooltip>

            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-9 w-9 text-primary-foreground hover:bg-primary-foreground/10"
                  onClick={handleCreateFile}
                >
                  <FilePlus className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>新建文件</TooltipContent>
            </Tooltip>
          </div>

          {/* 分隔线 + 返回聊天（对齐 dufs AppBar 右侧按钮区结构） */}
          <div className="w-px h-6 bg-primary-foreground/20 shrink-0" />
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                onClick={onBack}
                className="h-9 px-2.5 text-sm font-medium text-primary-foreground hover:bg-primary-foreground/10"
              >
                <ArrowLeft size={16} />
                <span className="hidden sm:inline">返回聊天</span>
              </Button>
            </TooltipTrigger>
            <TooltipContent>返回聊天</TooltipContent>
          </Tooltip>
        </header>

        {/* ===== 上传面板（常驻挂载，关闭时 hidden 保持上传进度；顶栏按钮触发文件/文件夹选择）===== */}
        <div className={`shrink-0 border-b border-border bg-card ${showUploader ? 'px-4 py-3' : 'hidden'}`}>
          <FileUploader
            basePath="/api/disks/default"
            targetDir={service.currentPath}
            onComplete={() => service.listDir(service.currentPath)}
            fileRequest={fileRequest}
            folderRequest={folderRequest}
          />
        </div>

        {/* ===== 列表区（flex-1 滚动，Card 包裹 FileBrowser 对齐 dufs 655-680）===== */}
        <div className="flex-1 min-h-0 overflow-y-auto">
          <div className="p-4 space-y-4">
            {/* README 卡片 — 当前目录存在 README 时显示在列表上方 */}
            {readmeEntry && !service.loading && !isSearching && (
              <ReadmeCard
                url={service.getPreviewUrl(entryPath(readmeEntry))}
                headers={getAuthHeaders()}
              />
            )}

            <Card className="py-0">
              <CardContent className="p-4">
                <FileBrowser
                  entries={service.entries}
                  total={service.total}
                  totalPages={service.totalPages}
                  page={service.page}
                  loading={service.loading}
                  error={service.error}
                  currentPath={service.currentPath}
                  sort={service.sort}
                  order={service.order}
                  onNavigate={service.navigateTo}
                  onSort={(field, order) => service.setSort(field, order)}
                  onPageChange={service.setPage}
                  onDoubleClick={handleDoubleClick}
                  onPreview={handlePreview}
                  onDownload={(entry) => window.open(service.getDownloadUrl(entryPath(entry)), '_blank')}
                  onDelete={handleDelete}
                  onRename={handleRename}
                  onZip={(entry) => window.open(service.getDownloadUrl(entryPath(entry), { zip: true }), '_blank')}
                  onCopyLink={handleCopyLink}
                />
              </CardContent>
            </Card>
          </div>
        </div>

        {/* ===== 底部 Footer：当前路径 + 条目数 ===== */}
        <footer className="shrink-0 px-4 py-2 border-t border-border bg-card flex items-center justify-between gap-3 text-xs text-muted-foreground">
          <span className="truncate">路径：{service.currentPath || '/'}</span>
          <span className="shrink-0 tabular-nums">
            {isSearching
              ? `搜索 "${service.searchQuery}" 结果 ${service.total} 项`
              : `共 ${service.total} 项`}
          </span>
        </footer>

        {/* ===== 预览（大尺寸 FilePreview 浮层）===== */}
        {previewEntry && (
          <FilePreview
            entry={previewEntry}
            previewUrl={service.getPreviewUrl(entryPath(previewEntry))}
            onClose={() => setPreviewEntry(null)}
            onNavigate={handlePreviewNavigate}
          />
        )}

        {/* ===== 内联文本编辑器（双击文本文件）===== */}
        {editEntry && (
          <EditDialog
            filename={editEntry.name}
            url={editUrl}
            initialContent={editContent}
            onSave={() => service.listDir(service.currentPath)}
            onClose={() => setEditEntry(null)}
          />
        )}

        {/* ===== 新建对话框（顶栏按钮预设类型，dufs FileListPage 311-363 对应）===== */}
        {showCreate && (
          <CreateDialog
            currentPath={service.currentPath}
            initialMode={createMode}
            onClose={() => setShowCreate(false)}
            onCreateDir={async (name) => {
              await service.createDir(name);
              setShowCreate(false);
            }}
            onCreateFile={async (name) => {
              await service.createFile(name);
              setShowCreate(false);
            }}
          />
        )}

        {/* ===== 重命名对话框 ===== */}
        {renameEntry && (
          <div
            className="fixed inset-0 z-[60] flex items-center justify-center bg-black/50"
            onClick={() => setRenameEntry(null)}
          >
            <div
              className="bg-white dark:bg-gray-900 rounded-xl shadow-2xl w-full max-w-sm p-5"
              onClick={(e) => e.stopPropagation()}
            >
              <h3 className="text-base font-semibold text-gray-900 dark:text-gray-100 mb-3">重命名</h3>
              <input
                autoFocus
                value={renameValue}
                onChange={(e) => setRenameValue(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && confirmRename()}
                className="w-full px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500"
              />
              <div className="flex items-center justify-end gap-2 mt-4">
                <button
                  onClick={() => setRenameEntry(null)}
                  className="px-3 py-1.5 text-sm rounded-lg border border-gray-300 dark:border-gray-600 hover:bg-gray-100 dark:hover:bg-gray-800"
                >
                  取消
                </button>
                <button
                  onClick={confirmRename}
                  className="px-3 py-1.5 text-sm rounded-lg bg-blue-600 text-white hover:bg-blue-700"
                >
                  重命名
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </TooltipProvider>
  );
}

export default FileManagerPage;