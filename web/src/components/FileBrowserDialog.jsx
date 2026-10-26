import React, { useState } from 'react';
import { X } from 'lucide-react';
import { FileBrowser, useFileService, FileBreadcrumb, SearchBar, FileUploader, FilePreview, CreateDialog, EditDialog, ReadmeCard } from '@actforce/file-manager';
import { getAuthHeaders } from '../lib/auth';

// Extensions that can be edited inline (mirrors dufs-astro-assets text preview set)
const TEXT_EXTS = new Set([
  'txt', 'md', 'markdown', 'json', 'csv', 'py', 'js', 'jsx', 'ts', 'tsx',
  'html', 'htm', 'css', 'scss', 'xml', 'yaml', 'yml', 'log', 'sql', 'sh',
  'ini', 'toml', 'conf', 'env', 'gitignore', 'svg',
]);

export function FileBrowserDialog({ open, onClose }) {
  const service = useFileService({ basePath: '/api/disks/default' });
  const [showUploader, setShowUploader] = useState(false);
  const [showCreate, setShowCreate] = useState(false);
  const [previewEntry, setPreviewEntry] = useState(null);
  const [editEntry, setEditEntry] = useState(null);
  const [editUrl, setEditUrl] = useState('');
  const [editContent, setEditContent] = useState('');
  const [renameEntry, setRenameEntry] = useState(null);
  const [renameValue, setRenameValue] = useState('');

  if (!open) return null;

  // Path of an entry relative to basePath (/disks/default)
  const entryPath = (entry) => {
    const dir = service.currentPath.replace(/\/+$/, '');
    return `${dir}/${entry.name}`;
  };

  const isTextFile = (entry) => {
    if (entry.is_dir) return false;
    const ext = (entry.name.split('.').pop() || '').toLowerCase();
    return TEXT_EXTS.has(ext);
  };

  // README entry in the current directory listing (readme.md / README.txt / readme.markdown / readme)
  const readmeEntry = service.entries.find(
    (e) => !e.is_dir && /^readme(\.md|\.txt|\.markdown)?$/i.test(e.name)
  );
  const isSearching = service.searchResults != null;

  const handlePreview = (entry) => {
    if (entry && !entry.is_dir) {
      setPreviewEntry(entry);
    }
  };

  // prev/next navigation across files in the current directory (FilePreview onNavigate)
  const handlePreviewNavigate = (delta) => {
    if (!previewEntry) return;
    const files = service.entries.filter((e) => !e.is_dir);
    if (files.length < 2) return;
    const idx = files.findIndex((e) => e.name === previewEntry.name);
    if (idx === -1) return;
    const next = files[(idx + delta + files.length) % files.length];
    setPreviewEntry(next);
  };

  // Double-click: folder → navigate; text file → inline editor; else → preview
  const handleDoubleClick = async (entry) => {
    if (entry.is_dir) {
      service.navigateTo(entryPath(entry));
      return;
    }
    if (isTextFile(entry)) {
      const url = service.getPreviewUrl(entryPath(entry));
      try {
        const res = await fetch(url, { headers: getAuthHeaders() });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const text = await res.text();
        setEditUrl(url);
        setEditContent(text);
        setEditEntry(entry);
      } catch (e) {
        console.error('Failed to load file for editing:', e);
      }
      return;
    }
    handlePreview(entry);
  };

  const handleRename = (entry) => {
    setRenameEntry(entry);
    setRenameValue(entry.name);
  };

  const confirmRename = async () => {
    const target = renameEntry;
    const newName = renameValue.trim();
    setRenameEntry(null);
    if (!target || !newName || newName === target.name) return;
    const dir = service.currentPath.replace(/\/+$/, '');
    try {
      await service.moveEntry(`${dir}/${target.name}`, `${dir}/${newName}`);
      service.listDir(service.currentPath);
    } catch (e) {
      console.error('Rename failed:', e);
    }
  };

  const handleCopyLink = async (entry) => {
    const url = service.getPreviewUrl(entryPath(entry));
    try {
      await navigator.clipboard.writeText(window.location.origin + url);
      console.log('Link copied');
    } catch (e) {
      console.error('Copy failed:', e);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-white dark:bg-gray-900 rounded-xl shadow-2xl w-full max-w-5xl h-[85vh] flex flex-col m-4">
        {/* Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-gray-700">
          <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">File Manager</h2>
          <div className="flex items-center gap-2">
            <button
              onClick={() => setShowCreate(true)}
              className="px-3 py-1.5 text-sm rounded-lg bg-blue-600 text-white hover:bg-blue-700"
            >
              + New
            </button>
            <button
              onClick={() => setShowUploader(!showUploader)}
              className="px-3 py-1.5 text-sm rounded-lg bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700"
            >
              Upload
            </button>
            <button
              onClick={onClose}
              className="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800"
            >
              <X size={18} />
            </button>
          </div>
        </div>

        {/* Breadcrumb + Search */}
        <div className="flex items-center gap-3 px-4 py-2 border-b border-gray-200 dark:border-gray-700">
          <FileBreadcrumb
            path={service.currentPath}
            onNavigate={service.navigateTo}
          />
          <div className="ml-auto w-64">
            <SearchBar
              value={service.searchQuery}
              onChange={service.debouncedSearch}
              onClear={() => service.setSearchQuery('')}
            />
          </div>
        </div>

        {/* Uploader panel — FileUploader expects basePath/targetDir/onComplete */}
        {showUploader && (
          <div className="px-4 py-3 border-b border-gray-200 dark:border-gray-700">
            <FileUploader
              basePath="/disks/default"
              targetDir={service.currentPath}
              onComplete={() => service.listDir(service.currentPath)}
            />
          </div>
        )}

        {/* Main file list */}
        <div className="flex-1 overflow-auto">
          {/* README card — shown above the list when the current directory has one */}
          {readmeEntry && !service.loading && !isSearching && (
            <div className="px-3 pt-3">
              <ReadmeCard
                url={service.getPreviewUrl(entryPath(readmeEntry))}
                headers={getAuthHeaders()}
              />
            </div>
          )}
          <FileBrowser
            entries={service.entries}
            total={service.total}
            totalPages={service.totalPages}
            page={service.page}
            loading={service.loading}
            error={service.error}
            currentPath={service.currentPath}
            sort={service.sort}
            order={service.order}
            onNavigate={service.navigateTo}
            onSort={(field, order) => service.setSort(field, order)}
            onPageChange={service.setPage}
            onDoubleClick={handleDoubleClick}
            onPreview={handlePreview}
            onDownload={(entry) => window.open(service.getDownloadUrl(entryPath(entry)), '_blank')}
            onDelete={async (entry) => {
              await service.deleteEntry(entryPath(entry));
            }}
            onRename={handleRename}
            onZip={(entry) => window.open(service.getDownloadUrl(entryPath(entry), { zip: true }), '_blank')}
            onCopyLink={handleCopyLink}
          />
        </div>
      </div>

      {/* Preview modal — FilePreview expects entry/previewUrl */}
      {previewEntry && (
        <FilePreview
          entry={previewEntry}
          previewUrl={service.getPreviewUrl(entryPath(previewEntry))}
          onClose={() => setPreviewEntry(null)}
          onNavigate={handlePreviewNavigate}
        />
      )}

      {/* Inline text editor (dufs-astro-assets EditDialog) */}
      {editEntry && (
        <EditDialog
          filename={editEntry.name}
          url={editUrl}
          initialContent={editContent}
          onSave={() => service.listDir(service.currentPath)}
          onClose={() => setEditEntry(null)}
        />
      )}

      {/* Create dialog */}
      {showCreate && (
        <CreateDialog
          onClose={() => setShowCreate(false)}
          onCreateDir={async (name) => {
            await service.createDir(name);
            setShowCreate(false);
          }}
          onCreateFile={async (name) => {
            await service.createFile(name);
            setShowCreate(false);
          }}
        />
      )}

      {/* Rename dialog */}
      {renameEntry && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/50" onClick={() => setRenameEntry(null)}>
          <div
            className="bg-white dark:bg-gray-900 rounded-xl shadow-2xl w-full max-w-sm p-5"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="text-base font-semibold text-gray-900 dark:text-gray-100 mb-3">Rename</h3>
            <input
              autoFocus
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && confirmRename()}
              className="w-full px-3 py-2 text-sm rounded-lg border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500"
            />
            <div className="flex items-center justify-end gap-2 mt-4">
              <button
                onClick={() => setRenameEntry(null)}
                className="px-3 py-1.5 text-sm rounded-lg border border-gray-300 dark:border-gray-600 hover:bg-gray-100 dark:hover:bg-gray-800"
              >
                Cancel
              </button>
              <button
                onClick={confirmRename}
                className="px-3 py-1.5 text-sm rounded-lg bg-blue-600 text-white hover:bg-blue-700"
              >
                Rename
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
