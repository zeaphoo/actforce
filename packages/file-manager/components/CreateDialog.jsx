import React, { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from './ui/dialog.jsx';
import { Button } from './ui/button.jsx';
import { Input } from './ui/input.jsx';

/**
 * 新建文件夹 / 新建文件对话框（shadcn Dialog）。
 * Props（向后兼容）：{ currentPath, onCreateDir, onCreateFile, onClose }
 * 附加可选 Props：initialMode = 'file' | 'dir'，用于顶栏“新建文件 / 新建文件夹”按钮预设类型。
 * - 宿主在 showCreate 时挂载本组件，故 Dialog 恒为打开态；关闭统一走 onOpenChange → onClose。
 */
export function CreateDialog({ currentPath, onCreateDir, onCreateFile, onClose, initialMode = 'file' }) {
  const { t } = useTranslation();
  const [mode, setMode] = useState(initialMode === 'dir' ? 'dir' : 'file');
  const [name, setName] = useState('');
  const [creating, setCreating] = useState(false);

  async function handleCreate() {
    if (!name.trim()) return;
    setCreating(true);
    try {
      if (mode === 'dir') {
        await onCreateDir?.(name.trim());
      } else {
        await onCreateFile?.(name.trim());
      }
      onClose?.();
    } catch (e) {
      alert(e.message);
    } finally {
      setCreating(false);
    }
  }

  function handleKeyDown(e) {
    if (e.key === 'Enter') handleCreate();
    // Escape 由 Dialog 处理（onOpenChange → onClose）
  }

  return (
    <Dialog open onOpenChange={(v) => { if (!v) onClose?.(); }}>
      <DialogContent className="max-w-sm sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {mode === 'dir' ? t('newFolder') : t('newFile')}
          </DialogTitle>
          <DialogDescription className="sr-only">
            {mode === 'dir' ? t('newFolder') : t('newFile')}
          </DialogDescription>
        </DialogHeader>

        {/* 文件类型选择（文件夹 / 文件） */}
        <div className="grid grid-cols-2 gap-2">
          <Button
            type="button"
            variant={mode === 'file' ? 'default' : 'outline'}
            size="sm"
            onClick={() => setMode('file')}
          >
            {t('newFile')}
          </Button>
          <Button
            type="button"
            variant={mode === 'dir' ? 'default' : 'outline'}
            size="sm"
            onClick={() => setMode('dir')}
          >
            {t('newFolder')}
          </Button>
        </div>

        <div className="space-y-1.5">
          <label className="text-sm font-medium leading-none">
            {mode === 'dir' ? t('folderName') : t('fileName')}
          </label>
          <Input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={mode === 'dir' ? 'new-folder' : 'new-file.txt'}
            autoFocus
          />
        </div>

        <p className="text-xs text-muted-foreground truncate" title={currentPath}>
          {currentPath}
        </p>

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t('cancel')}
          </Button>
          <Button onClick={handleCreate} disabled={!name.trim() || creating}>
            {creating ? t('uploading') : t('create')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}