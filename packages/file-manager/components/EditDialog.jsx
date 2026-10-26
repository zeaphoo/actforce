import React, { useState } from 'react';
import { X, WrapText } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { apiFetch } from '../lib/api';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './ui/dialog.jsx';
import { Button } from './ui/button.jsx';

/**
 * 文本内联编辑对话框（shadcn Dialog，视觉对齐 dufs EditDialog）。
 * Props（向后兼容）：{ filename, url, initialContent, onSave, onClose }
 * - 宿主在 editEntry 存在时挂载本组件，故 Dialog 恒为打开态；关闭统一走 onOpenChange → onClose。
 * - 保留：wrap 切换、未保存 ●、行数 / 字数统计、Ctrl+S 保存、Enter 无副作用。
 */
export function EditDialog({ filename, url, initialContent, onSave, onClose }) {
  const { t } = useTranslation();
  const [content, setContent] = useState(initialContent || '');
  const [saving, setSaving] = useState(false);
  const [wrapText, setWrapText] = useState(true);

  const hasChanges = content !== (initialContent || '');
  const lineCount = content.split('\n').length;
  const charCount = content.length;

  async function handleSave() {
    setSaving(true);
    try {
      const res = await apiFetch(url, {
        method: 'PUT',
        headers: { 'Content-Type': 'text/plain' },
        body: content,
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      onSave?.();
      onClose?.();
    } catch (e) {
      alert(e.message);
    } finally {
      setSaving(false);
    }
  }

  function handleKeyDown(e) {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault();
      handleSave();
    }
    // Escape 由 Dialog 处理（onOpenChange → onClose）
  }

  return (
    <Dialog open onOpenChange={(v) => { if (!v) onClose?.(); }}>
      <DialogContent
        showCloseButton={false}
        className="max-w-[min(960px,calc(100vw-32px))] sm:max-w-[min(960px,calc(100vw-32px))] max-h-[calc(100vh-32px)] flex flex-col p-0"
      >
        {/* 头部（对齐 dufs EditDialog：文件名 + wrap 切换 + 关闭） */}
        <DialogHeader className="px-4 py-3 border-b flex flex-row items-center gap-2 shrink-0">
          <DialogTitle className="flex-1 min-w-0 text-base font-medium truncate text-left flex items-center gap-1.5">
            {hasChanges && (
              <span className="text-primary shrink-0" title={t('statusModified')}>●</span>
            )}
            <span className="truncate">{t('edit')}: {filename}</span>
          </DialogTitle>
          <DialogDescription className="sr-only">
            {filename} editor
          </DialogDescription>

          {/* 自动换行切换 */}
          <Button
            variant={wrapText ? 'default' : 'ghost'}
            size="sm"
            className="h-8 gap-1"
            onClick={() => setWrapText(!wrapText)}
            title={t('wrapText')}
          >
            <WrapText size={16} />
            <span className="hidden sm:inline">{t('wrapText')}</span>
          </Button>

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

        {/* 编辑器区域 */}
        <div className="flex-1 min-h-0 p-4 overflow-hidden">
          <textarea
            value={content}
            onChange={(e) => setContent(e.target.value)}
            onKeyDown={handleKeyDown}
            wrap={wrapText ? 'soft' : 'off'}
            style={{
              whiteSpace: wrapText ? 'pre-wrap' : 'pre',
              wordBreak: wrapText ? 'break-word' : 'keep-all',
            }}
            className="w-full h-full min-h-[300px] resize-none border-0 bg-transparent font-mono text-sm leading-relaxed text-foreground focus:outline-none"
            spellCheck={false}
            autoFocus
          />
        </div>

        {/* 底部状态栏：快捷键提示 + 未保存 ● + 行数 / 字数 + 操作按钮 */}
        <div className="shrink-0 flex items-center justify-between gap-3 px-4 py-2 border-t border-border text-xs text-muted-foreground">
          <div className="flex items-center gap-3 min-w-0">
            <span className="whitespace-nowrap">{t('save')}: Ctrl+S</span>
            {hasChanges && (
              <span className="text-primary whitespace-nowrap">● {t('statusModified')}</span>
            )}
          </div>
          <div className="flex items-center gap-3 shrink-0">
            <div className="hidden sm:flex items-center gap-4 tabular-nums whitespace-nowrap">
              <span>{t('labelLines')}: {lineCount}</span>
              <span>{t('labelChars')}: {charCount}</span>
            </div>
            <Button variant="outline" size="sm" onClick={onClose}>
              {t('cancel')}
            </Button>
            <Button size="sm" onClick={handleSave} disabled={saving || !hasChanges}>
              {saving ? t('statusSaving') : t('save')}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}