import React, { useState, useCallback, useRef, useEffect } from 'react';
import { X } from 'lucide-react';
import { dialog } from '../lib/dialog.js';
import { cn } from '../lib/utils.js';

export function DialogProvider({ children }) {
  const [isOpen, setIsOpen] = useState(false);
  const [current, setCurrent] = useState(null);
  const [inputValue, setInputValue] = useState('');
  const queueRef = useRef([]);

  const processQueue = useCallback(() => {
    if (queueRef.current.length === 0 || isOpen) return;
    const next = queueRef.current.shift();
    setCurrent(next);
    setInputValue(next.defaultValue || '');
    setIsOpen(true);
  }, [isOpen]);

  useEffect(() => {
    const unsub = dialog.subscribe((config) => {
      queueRef.current.push(config);
      processQueue();
    });
    return unsub;
  }, [processQueue]);

  useEffect(() => {
    if (!isOpen) {
      const timer = setTimeout(() => processQueue(), 150);
      return () => clearTimeout(timer);
    }
  }, [isOpen, processQueue]);

  function handleConfirm() {
    if (!current) return;
    if (current.type === 'prompt') current.onResult(inputValue);
    else if (current.type === 'confirm') current.onResult(true);
    else current.onResult();
    setIsOpen(false);
  }

  function handleCancel() {
    if (!current) return;
    if (current.type === 'confirm') current.onResult(false);
    else if (current.type === 'prompt') current.onResult(null);
    else current.onResult();
    setIsOpen(false);
  }

  function getConfirmClass() {
    const map = { primary: 'bg-blue-600 hover:bg-blue-700', destructive: 'bg-red-600 hover:bg-red-700', success: 'bg-green-600 hover:bg-green-700', warning: 'bg-amber-600 hover:bg-amber-700' };
    return map[current?.confirmColor] || 'bg-blue-600 hover:bg-blue-700';
  }

  const showCancel = current?.type !== 'alert';

  return (
    <>
      {children}
      {isOpen && current && (
        <div className="fixed inset-0 z-[200] flex items-center justify-center bg-black/50" onClick={handleCancel}>
          <div
            className="bg-white dark:bg-gray-900 rounded-lg shadow-xl max-w-md w-full mx-4"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-gray-700">
              <h3 className="text-sm font-medium">{current.title || ''}</h3>
              <button onClick={handleCancel} className="p-1 text-gray-400 hover:text-gray-600 rounded">
                <X size={16} />
              </button>
            </div>

            <div className="px-4 py-3">
              {current.rawHtml ? (
                <div className="text-sm text-gray-600 dark:text-gray-400" dangerouslySetInnerHTML={{ __html: current.message }} />
              ) : (
                <p className="text-sm text-gray-600 dark:text-gray-400">{current.message}</p>
              )}

              {current.type === 'prompt' && (
                <div className="mt-3">
                  {current.type === 'prompt' && (
                    <textarea
                      value={inputValue}
                      onChange={(e) => setInputValue(e.target.value)}
                      onKeyDown={(e) => { if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); handleConfirm(); } }}
                      className={cn(
                        'w-full min-h-[100px] rounded border border-gray-300 dark:border-gray-600 bg-transparent px-3 py-2 text-sm',
                        'focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none'
                      )}
                      placeholder={current.placeholder || ''}
                      autoFocus
                    />
                  )}
                </div>
              )}
            </div>

            <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-gray-200 dark:border-gray-700">
              {showCancel && (
                <button onClick={handleCancel} className="px-3 py-1.5 text-xs border border-gray-300 dark:border-gray-600 rounded hover:bg-gray-50 dark:hover:bg-gray-800">
                  {current.cancelText || 'Cancel'}
                </button>
              )}
              <button onClick={handleConfirm} className={`px-3 py-1.5 text-xs text-white rounded ${getConfirmClass()}`}>
                {current.confirmText || 'OK'}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
