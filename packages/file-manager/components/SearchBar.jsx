import React, { useState } from 'react';
import { Search, X } from 'lucide-react';
import { debounce } from '../lib/utils.js';
import { useTranslation } from 'react-i18next';
import { Input } from './ui/input.jsx';
import { Button } from './ui/button.jsx';

/**
 * 搜索框：shadcn Input + 清除 Button。
 * Props 向后兼容：value / onChange / onSearch / placeholder，并支持 onClear（宿主用）。
 */
export function SearchBar({ value, onChange, onSearch, onClear, placeholder }) {
  const { t } = useTranslation();
  const [localValue, setLocalValue] = useState(value || '');

  const debouncedSearch = React.useMemo(
    () => debounce((q) => onSearch?.(q), 300),
    [onSearch]
  );

  function handleChange(e) {
    const v = e.target.value;
    setLocalValue(v);
    onChange?.(v);
    debouncedSearch(v);
  }

  function handleClear() {
    setLocalValue('');
    onChange?.('');
    onSearch?.('');
    onClear?.();
  }

  return (
    <div className="relative">
      <Search size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground" />
      <Input
        type="text"
        value={localValue}
        onChange={handleChange}
        placeholder={placeholder || t('search')}
        aria-label={placeholder || t('search')}
        className="pl-9 pr-9"
      />
      {localValue && (
        <Button
          variant="ghost"
          size="icon-xs"
          onClick={handleClear}
          aria-label={t('search')}
          className="absolute right-1 top-1/2 -translate-y-1/2 size-6 text-muted-foreground hover:text-foreground"
        >
          <X size={14} />
        </Button>
      )}
    </div>
  );
}