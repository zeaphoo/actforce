import React, { useMemo } from 'react';

export function Footer({ text, className = '' }) {
  const html = useMemo(() => {
    const raw = text || '';
    // Simple newline → <br> / link autodetection
    return raw
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener noreferrer" class="underline">$1</a>')
      .replace(/\n/g, '<br>');
  }, [text]);

  return (
    <footer
      className={`w-full max-w-[1080px] mx-auto px-4 py-4 text-center text-sm text-muted-foreground ${className}`}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
