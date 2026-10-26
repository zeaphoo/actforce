import React from 'react';
import { Home } from 'lucide-react';
import { cn } from '../lib/utils.js';
import { Button } from './ui/button.jsx';
import { Separator } from './ui/separator.jsx';

/**
 * 面包屑导航（视觉 shadcn 化：Home Button ghost + Separator 分隔 + hover:text-primary）。
 * Props（向后兼容）：{ path, onNavigate }
 */
export function FileBreadcrumb({ path, onNavigate }) {
  const parts = path.split('/').filter(Boolean);

  return (
    <nav className="flex items-center gap-1 text-sm text-muted-foreground min-w-0 flex-wrap">
      {/* Home 按钮 */}
      <Button
        variant="ghost"
        size="icon-sm"
        className="h-7 w-7 shrink-0 text-muted-foreground hover:text-primary"
        onClick={() => onNavigate('/')}
        title="/"
      >
        <Home size={14} />
      </Button>

      <Separator orientation="vertical" className="h-4 mx-0.5 shrink-0" />

      {parts.map((part, i) => {
        const fullPath = '/' + parts.slice(0, i + 1).join('/');
        const isLast = i === parts.length - 1;
        return (
          <React.Fragment key={fullPath}>
            <Button
              variant="ghost"
              size="sm"
              className={cn(
                'h-7 px-1.5 text-sm truncate max-w-[200px]',
                isLast
                  ? 'text-foreground font-medium'
                  : 'text-muted-foreground hover:text-primary hover:bg-transparent'
              )}
              onClick={() => onNavigate(fullPath)}
              title={fullPath}
            >
              {part}
            </Button>
            {!isLast && <Separator orientation="vertical" className="h-3.5 shrink-0" />}
          </React.Fragment>
        );
      })}
    </nav>
  );
}