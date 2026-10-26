// Zustand file service store
import { create } from 'zustand';
import { apiJson, apiFetch } from './api.js';

export const useFileStore = create((set, get) => ({
  // State
  entries: [],
  total: 0,
  totalPages: 1,
  loading: false,
  error: null,
  currentPath: '/',
  sort: 'name',
  order: 'asc',
  page: 1,
  limit: 100,
  searchQuery: '',
  searchResults: null,
  basePath: '/api/disks/default',

  // Set base path (for multi-disk support)
  setBasePath: (basePath) => set({ basePath }),

  // List directory
  listDir: async (dirPath, opts = {}) => {
    const { basePath, sort, order, limit } = get();
    set({ loading: true, error: null, searchResults: null });
    try {
      const s = opts.sort || sort;
      const o = opts.order || order;
      const p = opts.page || 1;
      const dir = dirPath || '/';
      const params = new URLSearchParams({ json: 'true', sort: s, order: o, page: String(p), limit: String(limit) });
      const data = await apiJson(`${basePath}${dir}?${params}`);
      set({
        entries: data.entries || [],
        total: data.total || 0,
        totalPages: data.total_pages ?? Math.ceil((data.total || 0) / limit),
        currentPath: dir,
        ...(opts.sort ? { sort: s } : {}),
        ...(opts.order ? { order: o } : {}),
        ...(opts.page ? { page: p } : {}),
      });
    } catch (e) {
      set({ error: e.message });
    } finally {
      set({ loading: false });
    }
  },

  // Search
  search: async (query) => {
    const { basePath, sort, order, currentPath } = get();
    if (!query.trim()) {
      set({ searchResults: null });
      get().listDir(currentPath);
      return;
    }
    set({ loading: true, error: null });
    try {
      const params = new URLSearchParams({ q: query, json: 'true', sort, order });
      const data = await apiJson(`${basePath}/?${params}`);
      set({
        searchResults: data,
        entries: data.entries || [],
        total: data.total || 0,
      });
    } catch (e) {
      set({ error: e.message });
    } finally {
      set({ loading: false });
    }
  },

  // Navigation
  navigateTo: (dirPath) => {
    set({ searchQuery: '', searchResults: null });
    get().listDir(dirPath);
  },

  goUp: () => {
    const { currentPath } = get();
    if (currentPath === '/') return;
    const parts = currentPath.split('/').filter(Boolean);
    parts.pop();
    const parent = parts.length ? '/' + parts.join('/') : '/';
    get().navigateTo(parent);
  },

  // Upload (single file, reference implementation — use createUploader for production)
  uploadFile: async (file, dirPath) => {
    const { basePath, currentPath } = get();
    const targetDir = dirPath || currentPath;
    const url = basePath + targetDir.replace(/\/+$/, '') + '/' + file.name;
    await apiFetch(url, { method: 'PUT', body: file });
    get().listDir(targetDir);
  },

  // Delete
  deleteEntry: async (entryPath) => {
    const { basePath, currentPath } = get();
    await apiFetch(basePath + entryPath, { method: 'DELETE' });
    get().listDir(currentPath);
  },

  // Create directory
  createDir: async (dirName) => {
    const { basePath, currentPath } = get();
    const url = basePath + currentPath.replace(/\/+$/, '') + '/' + dirName;
    await apiFetch(url, { method: 'MKCOL' });
    get().listDir(currentPath);
  },

  // Create empty file
  createFile: async (fileName) => {
    const { basePath, currentPath } = get();
    const url = basePath + currentPath.replace(/\/+$/, '') + '/' + fileName;
    await apiFetch(url, { method: 'PUT', body: '' });
    get().listDir(currentPath);
  },

  // Move/rename
  moveEntry: async (srcPath, destPath) => {
    const { basePath, currentPath } = get();
    await apiFetch(basePath + srcPath, {
      method: 'MOVE',
      headers: { Destination: encodeURI(basePath + destPath) },
    });
    get().listDir(currentPath);
  },

  // URL helpers
  getDownloadUrl: (entryPath, opts = {}) => {
    const { basePath } = get();
    const params = new URLSearchParams();
    if (opts.zip) params.set('zip', 'true');
    if (opts.hash) params.set('hash', 'true');
    const qs = params.toString();
    return basePath + entryPath + (qs ? '?' + qs : '');
  },

  getPreviewUrl: (entryPath) => {
    return get().basePath + entryPath;
  },

  // Sort/page setters
  setSort: (sort, order) => {
    set({ sort, order });
    get().listDir(get().currentPath, { sort, order });
  },

  setPage: (page) => {
    set({ page });
    get().listDir(get().currentPath, { page });
  },

  setSearchQuery: (query) => set({ searchQuery: query }),
}));
