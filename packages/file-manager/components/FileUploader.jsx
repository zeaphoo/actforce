import React, { useState, useRef, useEffect } from 'react';
import { Upload, FolderUp, Check, RotateCcw } from 'lucide-react';
import { cn } from '../lib/utils.js';
import { createUploader } from '../lib/uploader.js';
import { useTranslation } from 'react-i18next';
import { Button } from './ui/button.jsx';
import { Progress } from './ui/progress.jsx';

// 速度格式化：<1KB 显示 B，<1MB 显示 KB，否则 MB
function formatSpeed(bytesPerSec) {
  if (bytesPerSec == null || !isFinite(bytesPerSec) || bytesPerSec <= 0) return '0 B/s';
  if (bytesPerSec < 1024) return `${Math.round(bytesPerSec)} B/s`;
  if (bytesPerSec < 1024 * 1024) return `${(bytesPerSec / 1024).toFixed(1)} KB/s`;
  return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
}

/**
 * 文件 / 文件夹上传面板（视觉 shadcn 化：虚线拖拽区 + Button + Progress + 语义色）。
 * 上传逻辑（并发、去重、重试）保持不变。Props 向后兼容：{ basePath, targetDir, onComplete }
 * 附加可选 Props（宿主顶栏“上传文件 / 上传文件夹”按钮用，不传则完全无影响）：
 * - fileRequest：自增计数，每次变化时自动打开“选择文件”对话框（面板需已挂载，浏览器会在用户激活窗口内放行）
 * - folderRequest：自增计数，每次变化时自动打开“选择文件夹”对话框（webkitdirectory）
 */
export function FileUploader({ basePath, targetDir, onComplete, fileRequest = 0, folderRequest = 0 }) {
  const { t } = useTranslation();
  const [dragOver, setDragOver] = useState(false);
  const [uploads, setUploads] = useState([]);
  const inputRef = useRef(null);
  const folderInputRef = useRef(null);
  const uploaderRef = useRef(null);
  const prevFileRequest = useRef(0);
  const prevFolderRequest = useRef(0);

  // 外部“上传文件”按钮：请求打开文件选择器（仅在计数变化时触发一次，避免重挂载误弹）
  useEffect(() => {
    if (fileRequest > 0 && fileRequest !== prevFileRequest.current) {
      prevFileRequest.current = fileRequest;
      inputRef.current?.click();
    }
  }, [fileRequest]);

  // 外部“上传文件夹”按钮：请求打开文件夹选择器（webkitdirectory）
  useEffect(() => {
    if (folderRequest > 0 && folderRequest !== prevFolderRequest.current) {
      prevFolderRequest.current = folderRequest;
      folderInputRef.current?.click();
    }
  }, [folderRequest]);

  function getUploader() {
    if (!uploaderRef.current) {
      uploaderRef.current = createUploader({
        basePath,
        onProgress: (item) => {
          setUploads(prev => prev.map(u => (u.id === item.id ? { ...item } : u)));
        },
        onComplete: (item) => {
          setUploads(prev => prev.map(u => (u.id === item.id ? { ...item } : u)));
          onComplete?.();
        },
        onError: (item) => {
          setUploads(prev => prev.map(u => (u.id === item.id ? { ...item } : u)));
        },
      });
    }
    return uploaderRef.current;
  }

  async function enqueueFiles(input) {
    try {
      const created = await getUploader().enqueue(input, targetDir);
      if (created && created.length) {
        setUploads(prev => [...prev, ...created]);
      }
    } catch (err) {
      // 读取目录失败等异常（如目录权限），不崩溃
      console.error('FileUploader: failed to enqueue upload', err);
    }
  }

  function handleFileSelect(e) {
    if (e.target.files && e.target.files.length) enqueueFiles(e.target.files);
    e.target.value = '';
  }

  function handleFolderSelect(e) {
    if (e.target.files && e.target.files.length) enqueueFiles(e.target.files);
    e.target.value = '';
  }

  function handleDrop(e) {
    e.preventDefault();
    setDragOver(false);
    const items = e.dataTransfer.items;
    if (items && items.length > 0 && typeof items[0].webkitGetAsEntry === 'function') {
      // 用 items 遍历（支持文件夹拖拽，webkitGetAsEntry 递归读取）
      enqueueFiles(items);
    } else if (e.dataTransfer.files && e.dataTransfer.files.length) {
      // 不支持 webkitGetAsEntry 的浏览器降级为普通文件
      enqueueFiles(e.dataTransfer.files);
    }
  }

  async function retryItem(item) {
    try {
      const created = await getUploader().enqueue([{ path: item.path, file: item.file }], item.targetDir);
      if (created && created.length) {
        setUploads(prev => prev.map(u => (u.id === item.id ? created[0] : u)));
      }
    } catch (err) {
      console.error('FileUploader: failed to retry upload', err);
    }
  }

  const activeCount = uploads.filter(u => u.status === 'uploading' || u.status === 'pending').length;
  const successCount = uploads.filter(u => u.status === 'complete').length;
  const failedCount = uploads.filter(u => u.status === 'failed').length;
  const overallProgress = uploads.length > 0
    ? Math.round(uploads.reduce((sum, u) => sum + (u.progress || 0), 0) / uploads.length)
    : 0;

  return (
    <div className="relative">
      {/* 拖拽上传区（shadcn 语义色：border-primary / border-border） */}
      <div
        className={cn(
          'border-2 border-dashed rounded-lg p-4 sm:p-6 text-center transition-colors cursor-pointer',
          dragOver
            ? 'border-primary bg-primary/5'
            : 'border-border hover:border-muted-foreground/40'
        )}
        onDragOver={(e) => { e.preventDefault(); setDragOver(true); }}
        onDragLeave={() => setDragOver(false)}
        onDrop={handleDrop}
        onClick={() => inputRef.current?.click()}
      >
        <Upload size={32} className="mx-auto mb-2 text-muted-foreground/60" />
        <p className="text-sm text-muted-foreground">{t('dragAndDrop')}</p>
        <div className="mt-3 flex items-center justify-center gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={(e) => { e.stopPropagation(); inputRef.current?.click(); }}
          >
            <Upload size={14} />
            {t('uploadSelectFiles', { defaultValue: '选择文件' })}
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={(e) => { e.stopPropagation(); folderInputRef.current?.click(); }}
          >
            <FolderUp size={14} />
            {t('uploadSelectFolder', { defaultValue: '选择文件夹' })}
          </Button>
        </div>
        <input
          ref={inputRef}
          type="file"
          multiple
          className="hidden"
          onChange={handleFileSelect}
        />
        <input
          ref={folderInputRef}
          type="file"
          webkitdirectory=""
          directory=""
          multiple
          className="hidden"
          onChange={handleFolderSelect}
        />
      </div>

      {uploads.length > 0 && (
        <div className="mt-3 space-y-2">
          {/* 总进度（所有项平均，shadcn Progress） */}
          <div className="p-3 bg-muted/50 rounded-lg">
            <div className="flex items-center justify-between text-xs mb-1.5">
              <span className="text-muted-foreground">
                {activeCount > 0
                  ? t('uploadingCount', { defaultValue: '上传中 {{count}} 个', count: activeCount })
                  : t('uploadSummary', {
                      defaultValue: '成功 {{success}} 个，失败 {{failed}} 个',
                      success: successCount,
                      failed: failedCount,
                    })}
              </span>
              <span className="text-muted-foreground tabular-nums">{overallProgress}%</span>
            </div>
            <Progress value={overallProgress} />
          </div>

          {/* 上传项列表 */}
          <div className="max-h-60 overflow-y-auto space-y-2">
            {uploads.map((u) => {
              const failed = u.status === 'failed';
              return (
                <div
                  key={u.id}
                  className={cn(
                    'flex items-center gap-2 p-2 rounded-md text-sm border',
                    failed
                      ? 'border-destructive/40 bg-destructive/10 cursor-pointer hover:bg-destructive/15'
                      : 'border-transparent bg-muted/50'
                  )}
                  onClick={failed ? () => retryItem(u) : undefined}
                  title={failed ? t('uploadRetry', { defaultValue: '点击重试' }) : undefined}
                >
                  <span className="truncate flex-1 text-foreground">{u.path || u.file?.name || u.name}</span>
                  {u.status === 'uploading' && (
                    <span className="text-xs text-muted-foreground shrink-0">{formatSpeed(u.speed)}</span>
                  )}
                  <span className="text-xs w-14 text-right shrink-0">
                    {u.status === 'complete' ? (
                      <Check size={14} className="text-emerald-500 inline" />
                    ) : u.status === 'failed' ? (
                      <span className="inline-flex items-center gap-1 text-destructive">
                        <RotateCcw size={12} />
                        {t('uploadFailed')}
                      </span>
                    ) : u.status === 'cancelled' ? (
                      <span className="text-muted-foreground">{t('uploadCancelled', { defaultValue: '已取消' })}</span>
                    ) : (
                      `${u.progress}%`
                    )}
                  </span>
                  {(u.status === 'uploading' || u.status === 'pending') && (
                    <div className="w-20 shrink-0">
                      <Progress value={u.progress} className="h-1.5" />
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}