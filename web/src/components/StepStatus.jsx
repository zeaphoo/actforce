import { useState, useEffect, useRef } from 'react';

/**
 * StepStatus 组件 - 显示单个步骤状态
 */
function StepStatus({ status }) {
  // 图标映射
  const icons = {
    progress: (
      <svg className="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
        <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
        <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
      </svg>
    ),
    info: (
      <svg className="h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
    ),
    success: (
      <svg className="h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
      </svg>
    ),
    warning: (
      <svg className="h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
      </svg>
    ),
    error: (
      <svg className="h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
      </svg>
    ),
  };

  const colors = {
    progress: 'text-blue-600 dark:text-blue-400 bg-blue-50/80 dark:bg-blue-950/40 border-blue-200 dark:border-blue-800',
    info: 'text-gray-600 dark:text-gray-400 bg-gray-50/80 dark:bg-gray-900/40 border-gray-200 dark:border-gray-700',
    success: 'text-green-600 dark:text-green-400 bg-green-50/80 dark:bg-green-950/40 border-green-200 dark:border-green-800',
    warning: 'text-yellow-600 dark:text-yellow-400 bg-yellow-50/80 dark:bg-yellow-950/40 border-yellow-200 dark:border-yellow-800',
    error: 'text-red-600 dark:text-red-400 bg-red-50/80 dark:bg-red-950/40 border-red-200 dark:border-red-800',
  };

  const icon = icons[status.type] || icons.info;
  const colorClass = colors[status.type] || colors.info;

  return (
    <div className={`flex items-center gap-2 px-3 py-2 rounded-lg border ${colorClass} transition-all duration-300`}>
      <span className="flex-shrink-0">{icon}</span>
      <span className="text-sm flex-1">{status.content}</span>
      {status.progress !== undefined && status.progress !== null && (
        <div className="flex items-center gap-2 flex-shrink-0">
          <div className="w-24 h-1.5 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden">
            <div className="h-full bg-current transition-all duration-300" style={{ width: `${status.progress}%` }} />
          </div>
          <span className="text-xs font-medium min-w-[2.5rem] text-right">{status.progress}%</span>
        </div>
      )}
    </div>
  );
}

/**
 * StepStatusContainer 组件 - 单槽位显示 + 历史按钮
 */
export function StepStatusContainer({ statuses }) {
  const [showAll, setShowAll] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const sessionStart = useRef(null);
  const entries = Object.entries(statuses);

  // 记录首个 status 到达的时间，作为计时起点
  if (entries.length > 0 && sessionStart.current === null) {
    sessionStart.current = Date.now();
  } else if (entries.length === 0) {
    sessionStart.current = null;
  }

  // 每秒更新耗时
  useEffect(() => {
    if (sessionStart.current === null) return;
    const update = () => setElapsed(Math.floor((Date.now() - sessionStart.current) / 1000));
    update();
    const timer = setInterval(update, 1000);
    return () => clearInterval(timer);
  }, [sessionStart.current]);

  if (entries.length === 0) return null;

  const visibleEntries = showAll ? entries : [entries[entries.length - 1]];

  const fmt = (s) => s < 60 ? `${s}s` : `${Math.floor(s / 60)}m${s % 60}s`;

  return (
    <div className="space-y-2 mb-4">
      {visibleEntries.map(([id, status]) => (
        <StepStatus key={id} status={{ ...status, id }} />
      ))}
      <div className="flex items-center gap-2">
        <span className="text-xs text-muted-foreground">耗时{fmt(elapsed)}</span>
        {entries.length > 1 && (
          <button
            onClick={() => setShowAll(!showAll)}
            className="text-xs text-muted-foreground hover:text-foreground transition-colors"
          >
            {showAll ? '收起' : `显示全部 (${entries.length})`}
          </button>
        )}
      </div>
    </div>
  );
}
