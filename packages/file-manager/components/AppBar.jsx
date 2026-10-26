import React from 'react';
import { Sun, Moon, LogIn, LogOut } from 'lucide-react';
import { Button } from './ui/button.jsx';
import {
  TooltipProvider, Tooltip, TooltipTrigger, TooltipContent,
} from './ui/tooltip.jsx';

/**
 * 顶栏 — shadcn 风格（bg-secondary / border-b / shadow），对齐 dufs AppBar。
 * Props 与旧版完全兼容。
 */
export function AppBar({
  title,
  logoUrl,
  theme = 'light',
  user = null,
  authEnabled = false,
  onToggleTheme,
  onLogin,
  onLogout,
  children,
  breadcrumb,
  actions,
}) {
  return (
    <TooltipProvider delayDuration={300}>
      <header
        className="sticky top-0 z-50 w-full h-12 flex items-center px-4 gap-2 border-b bg-secondary text-secondary-foreground shrink-0"
        style={{ boxShadow: '0 2px 8px 0px hsl(0 0% 10.1961% / 0.15)' }}
      >
        {/* Left: Logo + Title */}
        <div className="flex items-center gap-2 shrink-0">
          {logoUrl && (
            <img src={logoUrl} alt="logo" className="h-8 w-auto" style={{ marginLeft: '4px' }} />
          )}
          {title && (
            <span className="text-base font-semibold whitespace-nowrap">{title}</span>
          )}
        </div>

        {/* Center: Breadcrumb */}
        <div className="flex items-center gap-1 flex-1 min-w-0">
          {breadcrumb}
        </div>

        {/* Actions (upload, new folder, etc.) */}
        <div className="flex items-center gap-1 shrink-0">
          {actions}
        </div>

        {/* Separator */}
        <div className="w-px h-6 bg-primary-foreground/20 shrink-0" />

        {/* Right: Auth + Theme */}
        <div className="flex items-center gap-1 shrink-0">
          {authEnabled && (
            <>
              {user ? (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={onLogout}
                      className="h-9 w-9 text-primary-foreground hover:bg-primary-foreground/10"
                    >
                      <LogOut size={16} />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>{user}</TooltipContent>
                </Tooltip>
              ) : (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={onLogin}
                      className="h-9 w-9 text-primary-foreground hover:bg-primary-foreground/10"
                    >
                      <LogIn size={16} />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>Login</TooltipContent>
                </Tooltip>
              )}
            </>
          )}

          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                onClick={onToggleTheme}
                className="h-9 w-9 text-primary-foreground hover:bg-primary-foreground/10"
                title={theme === 'dark' ? 'Light mode' : 'Dark mode'}
              >
                {theme === 'dark' ? <Sun size={18} /> : <Moon size={18} />}
              </Button>
            </TooltipTrigger>
            <TooltipContent>{theme === 'dark' ? 'Light mode' : 'Dark mode'}</TooltipContent>
          </Tooltip>
        </div>

        {children}
      </header>
    </TooltipProvider>
  );
}