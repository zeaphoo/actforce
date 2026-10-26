import React from 'react';
import { Pencil, Link, Archive, Download, Trash2, MoreVertical } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from './ui/dropdown-menu.jsx';
import { Button } from './ui/button.jsx';

/**
 * 每行操作：shadcn DropdownMenu（Trigger 为 MoreVertical 幽灵按钮），
 * 菜单结构对齐 dufs FileList 的操作菜单（重命名 / 复制链接 / 下载 ZIP / 下载 / 删除）。
 * Props 与旧版完全兼容：entry / onDownload / onDelete / onRename / onCopyLink / onZip。
 */
export function FileActions({ entry, onDownload, onDelete, onRename, onCopyLink, onZip }) {
  const { t } = useTranslation();

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon-sm" className="size-8">
          <MoreVertical className="size-4" />
          <span className="sr-only">{t('actions')}</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onClick={() => onRename?.(entry)}>
          <Pencil className="size-4" />
          {t('rename')}
        </DropdownMenuItem>
        <DropdownMenuItem onClick={() => onCopyLink?.(entry)}>
          <Link className="size-4" />
          {t('copyLink')}
        </DropdownMenuItem>

        {(entry.is_dir ? onZip : onDownload) && <DropdownMenuSeparator />}
        {entry.is_dir
          ? onZip && (
              <DropdownMenuItem onClick={() => onZip?.(entry)}>
                <Archive className="size-4" />
                {t('downloadZip')}
              </DropdownMenuItem>
            )
          : onDownload && (
              <DropdownMenuItem onClick={() => onDownload?.(entry)}>
                <Download className="size-4" />
                {t('download')}
              </DropdownMenuItem>
            )}

        <DropdownMenuSeparator />
        <DropdownMenuItem variant="destructive" onClick={() => onDelete?.(entry)}>
          <Trash2 className="size-4" />
          {t('delete')}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}