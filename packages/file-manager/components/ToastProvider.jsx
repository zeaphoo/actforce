import React, { useState, useEffect, useCallback } from 'react';
import { X, CheckCircle, AlertCircle, Info, AlertTriangle } from 'lucide-react';
import { toast } from '../lib/toast.js';
import { cn } from '../lib/utils.js';

export function ToastProvider({ children }) {
  const [toasts, setToasts] = useState([]);

  useEffect(() => {
    const unsubscribe = toast.subscribe((config) => {
      if (config.dismiss) {
        setToasts(prev => prev.filter(t => t.id !== config.id));
      } else {
        setToasts(prev => {
          const next = [...prev, config];
          return next.length > 5 ? next.slice(next.length - 5) : next;
        });
      }
    });
    return unsubscribe;
  }, []);

  const removeToast = useCallback((id) => {
    setToasts(prev => prev.filter(t => t.id !== id));
  }, []);

  return (
    <>
      {children}
      <ToastContainer toasts={toasts} onRemove={removeToast} />
    </>
  );
}

function ToastContainer({ toasts, onRemove }) {
  return (
    <div className="fixed bottom-4 right-4 z-[100] flex flex-col gap-2">
      {toasts.map(toast => (
        <ToastItem key={toast.id} toast={toast} onRemove={onRemove} />
      ))}
    </div>
  );
}

function ToastItem({ toast: t, onRemove }) {
  const [exiting, setExiting] = useState(false);

  useEffect(() => {
    const timer = setTimeout(() => {
      setExiting(true);
      setTimeout(() => onRemove(t.id), 300);
    }, t.duration);
    return () => clearTimeout(timer);
  }, [t.id, t.duration, onRemove]);

  const iconMap = {
    success: <CheckCircle size={20} className="text-white" />,
    error: <AlertCircle size={20} className="text-white" />,
    warning: <AlertTriangle size={20} className="text-white" />,
    info: <Info size={20} className="text-white" />,
  };

  const bgMap = {
    success: 'bg-green-500',
    error: 'bg-red-500',
    warning: 'bg-amber-500',
    info: 'bg-blue-500',
  };

  return (
    <div
      className={cn(
        'flex items-center gap-3 rounded-lg px-4 py-3 shadow-lg min-w-[300px] max-w-[400px] transition-all duration-300',
        bgMap[t.type] || 'bg-blue-500',
        exiting ? 'opacity-0 translate-x-full' : 'opacity-100'
      )}
      role="alert"
    >
      <div className="shrink-0">{iconMap[t.type] || iconMap.info}</div>
      <div className="flex-1 text-sm font-medium text-white">{t.message}</div>
      {t.action && (
        <button
          onClick={() => { t.action.onClick?.(); onRemove(t.id); }}
          className="shrink-0 text-sm font-medium text-white/90 hover:text-white underline"
        >
          {t.action.label}
        </button>
      )}
      <button
        onClick={() => { setExiting(true); setTimeout(() => onRemove(t.id), 300); }}
        className="shrink-0 rounded p-1 text-white/70 hover:text-white hover:bg-white/20"
      >
        <X size={16} />
      </button>
    </div>
  );
}
