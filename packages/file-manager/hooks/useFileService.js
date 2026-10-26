import { useFileStore } from '../lib/fileStore.js';
import { useEffect } from 'react';

export function useFileService({ basePath = '/api/disks/default' } = {}) {
  const store = useFileStore();

  useEffect(() => {
    store.setBasePath(basePath);
  }, [basePath]);

  useEffect(() => {
    store.listDir(store.currentPath);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  return {
    entries: store.entries,
    total: store.total,
    loading: store.loading,
    error: store.error,
    currentPath: store.currentPath,
    sort: store.sort,
    order: store.order,
    page: store.page,
    searchQuery: store.searchQuery,
    searchResults: store.searchResults,
    setSearchQuery: store.setSearchQuery,
    setSort: store.setSort,
    setOrder: (order) => store.setSort(store.sort, order),
    listDir: store.listDir,
    search: store.search,
    debouncedSearch: (query) => store.search(query),
    navigateTo: store.navigateTo,
    goUp: store.goUp,
    uploadFile: store.uploadFile,
    deleteEntry: store.deleteEntry,
    createDir: store.createDir,
    createFile: store.createFile,
    moveEntry: store.moveEntry,
    getDownloadUrl: store.getDownloadUrl,
    getPreviewUrl: store.getPreviewUrl,
  };
}
