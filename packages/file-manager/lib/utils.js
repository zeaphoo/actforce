import { clsx } from 'clsx';
import { twMerge } from 'tailwind-merge';

export function cn(...inputs) {
  return twMerge(clsx(inputs));
}

const KB = 1024;
const MB = KB * 1024;
const GB = MB * 1024;
const TB = GB * 1024;

export function formatSize(bytes) {
  if (bytes == null || bytes < 0) return '--';
  if (bytes === 0) return '0 B';
  if (bytes < KB) return bytes + ' B';
  if (bytes < MB) return (bytes / KB).toFixed(1) + ' KB';
  if (bytes < GB) return (bytes / MB).toFixed(1) + ' MB';
  if (bytes < TB) return (bytes / GB).toFixed(2) + ' GB';
  return (bytes / TB).toFixed(2) + ' TB';
}

export function formatDate(timestamp) {
  if (!timestamp) return '--';
  const d = new Date(timestamp * 1000);
  const now = new Date();
  const diff = now - d;
  if (diff < 60_000) return 'just now';
  if (diff < 3_600_000) return Math.floor(diff / 60_000) + 'm ago';
  if (diff < 86_400_000) return Math.floor(diff / 3_600_000) + 'h ago';
  if (d.getFullYear() === now.getFullYear()) {
    return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  }
  return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
}

export function debounce(fn, ms) {
  let timer;
  return (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
}

export function extname(filename) {
  const i = filename.lastIndexOf('.');
  return i > 0 ? filename.slice(i + 1).toLowerCase() : '';
}

export function basename(path) {
  return path.split('/').filter(Boolean).pop() || '';
}

export function dirname(path) {
  const parts = path.split('/').filter(Boolean);
  parts.pop();
  return '/' + parts.join('/');
}

const TEXT_EXTS = new Set([
  'txt', 'md', 'markdown', 'json', 'xml', 'yaml', 'yml', 'toml', 'ini', 'cfg',
  'conf', 'log', 'csv', 'tsv', 'html', 'htm', 'css', 'js', 'jsx', 'ts', 'tsx',
  'py', 'rs', 'go', 'java', 'c', 'cpp', 'h', 'hpp', 'sh', 'bash', 'zsh', 'fish',
  'sql', 'r', 'rb', 'php', 'lua', 'vim', 'env', 'gitignore', 'dockerfile',
]);

export function isTextFile(filename) {
  const ext = extname(filename);
  if (ext) return TEXT_EXTS.has(ext);
  const base = filename.toLowerCase();
  return ['dockerfile', 'makefile', 'license', 'readme'].some(n => base.startsWith(n));
}

const IMAGE_EXTS = new Set(['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp', 'ico', 'avif']);
const VIDEO_EXTS = new Set(['mp4', 'webm', 'ogg', 'mov', 'avi', 'mkv']);
const AUDIO_EXTS = new Set(['mp3', 'wav', 'flac', 'aac', 'ogg', 'opus', 'm4a']);
const OFFICE_EXTS = new Set(['docx', 'xlsx', 'pptx', 'doc', 'xls', 'ppt', 'pdf']);

export function fileCategory(filename) {
  const ext = extname(filename);
  if (IMAGE_EXTS.has(ext)) return 'image';
  if (VIDEO_EXTS.has(ext)) return 'video';
  if (AUDIO_EXTS.has(ext)) return 'audio';
  if (OFFICE_EXTS.has(ext)) return 'office';
  if (TEXT_EXTS.has(ext)) return 'text';
  if (ext === 'zip') return 'archive';
  return 'binary';
}

export function mimeType(filename) {
  const ext = extname(filename);
  const map = {
    jpg: 'image/jpeg', jpeg: 'image/jpeg', png: 'image/png', gif: 'image/gif',
    webp: 'image/webp', svg: 'image/svg+xml', bmp: 'image/bmp', ico: 'image/x-icon',
    avif: 'image/avif', mp4: 'video/mp4', webm: 'video/webm', ogg: 'audio/ogg',
    mov: 'video/quicktime', avi: 'video/x-msvideo', mkv: 'video/x-matroska',
    mp3: 'audio/mpeg', wav: 'audio/wav', flac: 'audio/flac', aac: 'audio/aac',
    opus: 'audio/opus', m4a: 'audio/mp4', pdf: 'application/pdf',
    doc: 'application/msword', docx: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    xls: 'application/vnd.ms-excel', xlsx: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    ppt: 'application/vnd.ms-powerpoint', pptx: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
    zip: 'application/zip', json: 'application/json', html: 'text/html', htm: 'text/html',
    css: 'text/css', js: 'text/javascript', md: 'text/markdown', txt: 'text/plain',
    xml: 'application/xml', csv: 'text/csv',
  };
  return map[ext] || 'application/octet-stream';
}
