import React, { useState, useEffect, useRef } from 'react';
import { FileText } from 'lucide-react';
import { marked } from 'marked';
import Prism from 'prismjs';

// ---------------------------------------------------------------------------
// Prism language components (order matters: dependencies must come first)
// ---------------------------------------------------------------------------
import 'prismjs/components/prism-clike.min.js'; // required by many languages below
import 'prismjs/components/prism-markup.min.js'; // html / xml / svg aliases
import 'prismjs/components/prism-markup-templating.min.js'; // required by php
import 'prismjs/components/prism-javascript.min.js'; // js alias
import 'prismjs/components/prism-python.min.js'; // py alias
import 'prismjs/components/prism-typescript.min.js'; // ts alias
import 'prismjs/components/prism-jsx.min.js';
import 'prismjs/components/prism-tsx.min.js';
import 'prismjs/components/prism-json.min.js';
import 'prismjs/components/prism-css.min.js';
import 'prismjs/components/prism-sql.min.js';
import 'prismjs/components/prism-yaml.min.js'; // yml alias
import 'prismjs/components/prism-java.min.js';
import 'prismjs/components/prism-c.min.js';
import 'prismjs/components/prism-cpp.min.js';
import 'prismjs/components/prism-csharp.min.js';
import 'prismjs/components/prism-go.min.js';
import 'prismjs/components/prism-rust.min.js'; // rs alias
import 'prismjs/components/prism-ruby.min.js'; // rb alias
import 'prismjs/components/prism-php.min.js';
import 'prismjs/components/prism-bash.min.js'; // sh / shell aliases
import 'prismjs/components/prism-docker.min.js'; // dockerfile alias
import 'prismjs/components/prism-toml.min.js';
import 'prismjs/components/prism-diff.min.js'; // patch alias
import 'prismjs/components/prism-markdown.min.js'; // md alias

export { Prism };

// ---------------------------------------------------------------------------
// Extension → Prism language map (used for plain code-file preview)
// ---------------------------------------------------------------------------
export const CODE_LANG_BY_EXT = {
  // web
  js: 'javascript', mjs: 'javascript', cjs: 'javascript', jsx: 'jsx',
  ts: 'typescript', tsx: 'tsx',
  json: 'json', jsonc: 'json',
  html: 'html', htm: 'html', xml: 'xml', svg: 'svg',
  css: 'css', scss: 'css', less: 'css',
  // scripting
  py: 'python', pyi: 'python', pyw: 'python',
  rb: 'ruby', rake: 'ruby', gemfile: 'ruby',
  sh: 'bash', bash: 'bash', zsh: 'bash', fish: 'bash',
  ps1: 'powershell', psd1: 'powershell', psm1: 'powershell',
  lua: 'lua', pl: 'perl', pm: 'perl', r: 'r',
  php: 'php', http: 'http', graphql: 'graphql', gql: 'graphql',
  // systems
  go: 'go', rs: 'rust', java: 'java', kt: 'kotlin', kts: 'kotlin',
  swift: 'swift', dart: 'dart', scala: 'scala',
  c: 'c', h: 'c', cc: 'cpp', cpp: 'cpp', cxx: 'cpp', hpp: 'cpp', hh: 'cpp',
  cs: 'csharp', sol: 'solidity',
  // data / config
  sql: 'sql', yml: 'yaml', yaml: 'yaml', toml: 'toml',
  ini: 'ini', cfg: 'ini', conf: 'ini', properties: 'properties',
  csv: 'csv', proto: 'protobuf',
  // tooling
  dockerfile: 'docker', makefile: 'makefile', make: 'makefile',
  nginx: 'nginx', vim: 'vim', vimrc: 'vim',
  // docs / other
  md: 'markdown', markdown: 'markdown', diff: 'diff', patch: 'diff',
};

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------
function escapeHtml(str) {
  return String(str ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function normalizeLang(lang) {
  const aliases = { 'c++': 'cpp', 'c#': 'csharp', 'shell': 'bash' };
  return aliases[lang] || lang;
}

/**
 * Build the HTML for a highlighted-ready <pre> code block.
 * The code is escaped; call applyCodeHighlight() on the container afterwards
 * to run Prism highlighting and inject line-number rows.
 */
export function buildCodeBlockHtml(code, lang) {
  const raw = (lang || '').split(/\s+/)[0].toLowerCase();
  const normalized = normalizeLang(raw);
  const langCls = normalized && Prism.languages[normalized]
    ? `language-${normalized}`
    : 'language-plaintext';
  const text = escapeHtml(String(code ?? '').replace(/\n$/, ''));
  return `<pre class="line-numbers ${langCls}"><code class="${langCls}">${text}\n</code></pre>`;
}

/**
 * Highlight every `pre.line-numbers > code[class*="language-"]` inside the
 * container and inject `.line-numbers-rows` (CSS-counter based line numbers).
 */
export function applyCodeHighlight(container) {
  if (!container || typeof container.querySelectorAll !== 'function') return;
  container.querySelectorAll('pre.line-numbers > code[class*="language-"]').forEach((codeEl) => {
    // Highlight (safe: falls back to the escaped text when the grammar is missing)
    try {
      Prism.highlightElement(codeEl);
    } catch {
      /* keep the escaped source */
    }
    // Line numbers (simple CSS-counter rows, no plugin needed)
    const pre = codeEl.parentElement;
    if (!pre || pre.querySelector('.line-numbers-rows')) return;
    const numLines = (codeEl.textContent.match(/\n(?!$)/g) || []).length + 1;
    const rows = document.createElement('span');
    rows.setAttribute('aria-hidden', 'true');
    rows.className = 'line-numbers-rows';
    rows.innerHTML = new Array(numLines + 1).join('<span></span>');
    codeEl.appendChild(rows);
  });
}

// ---------------------------------------------------------------------------
// Marked configuration (per-parse options, no global mutation)
// ---------------------------------------------------------------------------
const renderer = new marked.Renderer();
renderer.code = function code(token) {
  return buildCodeBlockHtml(token.text, token.lang);
};

const MARKED_OPTIONS = {
  gfm: true,
  breaks: true,
  renderer,
};

// Scoped styles: prism token colors + line numbers + dark-mode prose fixes.
// Scoped under `.fm-markdown` so they never leak outside this component.
const PREVIEW_STYLES = `
.fm-markdown code[class*="language-"], .fm-markdown pre[class*="language-"] {
  color: #24292f;
  background: none;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
  font-size: 0.85em;
  text-align: left;
  white-space: pre;
  word-spacing: normal;
  word-break: normal;
  word-wrap: normal;
  line-height: 1.6;
  -moz-tab-size: 4;
  -o-tab-size: 4;
  tab-size: 4;
  -webkit-hyphens: none;
  -moz-hyphens: none;
  -ms-hyphens: none;
  hyphens: none;
}
.fm-markdown pre[class*="language-"] {
  background: #f6f8fa;
  border: 1px solid rgba(128,128,128,0.18);
  border-radius: 0.5rem;
  margin: 1rem 0;
  padding: 1rem;
  overflow: auto;
}
.fm-markdown pre[class*="language-"] code {
  display: block;
}
/* line numbers */
.fm-markdown pre.line-numbers { position: relative; padding-left: 3.8em; counter-reset: linenumber; }
.fm-markdown pre.line-numbers > code { position: relative; white-space: inherit; }
.fm-markdown .line-numbers-rows {
  position: absolute;
  pointer-events: none;
  top: 0;
  font-size: 100%;
  left: -3.4em;
  width: 3em;
  letter-spacing: -1px;
  border-right: 1px solid rgba(128,128,128,0.25);
  -webkit-user-select: none;
  -moz-user-select: none;
  -ms-user-select: none;
  user-select: none;
}
.fm-markdown .line-numbers-rows > span { display: block; counter-increment: linenumber; }
.fm-markdown .line-numbers-rows > span::before {
  content: counter(linenumber);
  color: #9ca3af;
  display: block;
  padding-right: 0.8em;
  text-align: right;
}
/* inline code (no backtick pseudo chars from prose) */
.fm-markdown code:not([class*="language-"]) {
  background: rgba(127,127,127,0.12);
  padding: 0.15em 0.35em;
  border-radius: 0.25rem;
  font-size: 0.875em;
  color: #b3261e;
}
.fm-markdown code::before, .fm-markdown code::after { content: none; }
/* light token colors (GitHub-ish) */
.fm-markdown .token.comment, .fm-markdown .token.prolog, .fm-markdown .token.doctype, .fm-markdown .token.cdata { color: #6e7781; }
.fm-markdown .token.punctuation { color: #24292f; }
.fm-markdown .token.property, .fm-markdown .token.tag, .fm-markdown .token.boolean, .fm-markdown .token.number,
.fm-markdown .token.constant, .fm-markdown .token.symbol, .fm-markdown .token.deleted { color: #cf222e; }
.fm-markdown .token.selector, .fm-markdown .token.attr-name, .fm-markdown .token.string, .fm-markdown .token.char,
.fm-markdown .token.builtin, .fm-markdown .token.inserted { color: #0a3069; }
.fm-markdown .token.operator, .fm-markdown .token.entity, .fm-markdown .token.url,
.fm-markdown .language-css .token.string, .fm-markdown .style .token.string { color: #8250df; }
.fm-markdown .token.atrule, .fm-markdown .token.attr-value, .fm-markdown .token.keyword { color: #cf222e; }
.fm-markdown .token.function, .fm-markdown .token.class-name { color: #8250df; }
.fm-markdown .token.regex, .fm-markdown .token.important, .fm-markdown .token.variable { color: #953800; }
/* dark mode */
.dark .fm-markdown code[class*="language-"], .dark .fm-markdown pre[class*="language-"] { color: #e5e7eb; }
.dark .fm-markdown pre[class*="language-"] { background: #1f2937; border-color: rgba(128,128,128,0.2); }
.dark .fm-markdown .line-numbers-rows { border-right-color: rgba(128,128,128,0.3); }
.dark .fm-markdown .line-numbers-rows > span::before { color: #6b7280; }
.dark .fm-markdown code:not([class*="language-"]) { background: rgba(127,127,127,0.22); color: #f472b6; }
.dark .fm-markdown .token.comment, .dark .fm-markdown .token.prolog, .dark .fm-markdown .token.doctype, .dark .fm-markdown .token.cdata { color: #8b949e; }
.dark .fm-markdown .token.punctuation { color: #c9d1d9; }
.dark .fm-markdown .token.property, .dark .fm-markdown .token.tag, .dark .fm-markdown .token.boolean, .dark .fm-markdown .token.number,
.dark .fm-markdown .token.constant, .dark .fm-markdown .token.symbol, .dark .fm-markdown .token.deleted { color: #ff7b72; }
.dark .fm-markdown .token.selector, .dark .fm-markdown .token.attr-name, .dark .fm-markdown .token.string, .dark .fm-markdown .token.char,
.dark .fm-markdown .token.builtin, .dark .fm-markdown .token.inserted { color: #a5d6ff; }
.dark .fm-markdown .token.operator, .dark .fm-markdown .token.entity, .dark .fm-markdown .token.url,
.dark .fm-markdown .language-css .token.string, .dark .fm-markdown .style .token.string { color: #d2a8ff; }
.dark .fm-markdown .token.atrule, .dark .fm-markdown .token.attr-value, .dark .fm-markdown .token.keyword { color: #ff7b72; }
.dark .fm-markdown .token.function, .dark .fm-markdown .token.class-name { color: #d2a8ff; }
.dark .fm-markdown .token.regex, .dark .fm-markdown .token.important, .dark .fm-markdown .token.variable { color: #ffa657; }
/* dark-mode prose overrides (in case dark:prose-invert is not generated) */
.dark .fm-markdown { color: #d1d5db; }
.dark .fm-markdown h1, .dark .fm-markdown h2, .dark .fm-markdown h3,
.dark .fm-markdown h4, .dark .fm-markdown h5, .dark .fm-markdown h6 { color: #f3f4f6; }
.dark .fm-markdown a { color: #93c5fd; }
.dark .fm-markdown blockquote { color: #9ca3af; border-left-color: #4b5563; }
.dark .fm-markdown hr { border-color: #374151; }
.dark .fm-markdown th, .dark .fm-markdown td { border-color: #374151; }
.dark .fm-markdown strong { color: #f3f4f6; }
`;

/**
 * Markdown renderer with GFM + line-numbered, Prism-highlighted code blocks.
 * Props: { content, className }
 */
export function MarkdownRenderer({ content, className = '' }) {
  const containerRef = useRef(null);
  const [html, setHtml] = useState('');
  const [error, setError] = useState(null);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    try {
      const out = marked.parse(content || '', MARKED_OPTIONS);
      if (!cancelled) setHtml(out);
    } catch (e) {
      if (!cancelled) setError(e?.message || 'Failed to render markdown');
    }
    return () => { cancelled = true; };
  }, [content]);

  // Highlight code blocks + inject line numbers after the HTML is committed
  useEffect(() => {
    if (!html || !containerRef.current) return;
    applyCodeHighlight(containerRef.current);
  }, [html]);

  if (error) {
    return (
      <div className="text-destructive text-sm px-4 py-2">
        <FileText size={14} className="inline mr-1" />
        {error}
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className={`fm-markdown markdown-body prose prose-sm max-w-none dark:prose-invert ${className}`}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

// Inject the scoped preview styles once per document
if (typeof document !== 'undefined' && !document.getElementById('fm-markdown-styles')) {
  const style = document.createElement('style');
  style.id = 'fm-markdown-styles';
  style.textContent = PREVIEW_STYLES;
  document.head.appendChild(style);
}
