// XHR-based file uploader with progress tracking, speed calculation and directory upload support
import { getToken } from './api.js';

/**
 * 递归读取 FileSystemEntry（文件/目录），返回 [{ path, file }]
 * - 目录的 readEntries 需要多次调用直到返回空数组才能拿到全部条目
 * - path 携带相对路径（如 "folder/sub/file.txt"）
 */
function readEntry(entry, path = '') {
  return new Promise((resolve, reject) => {
    if (entry.isFile) {
      entry.file(
        (file) => resolve([{ file, path: path + file.name }]),
        reject,
      );
    } else if (entry.isDirectory) {
      const reader = entry.createReader();
      const readBatch = () =>
        new Promise((res, rej) => reader.readEntries(res, rej));
      (async () => {
        try {
          const entries = [];
          while (true) {
            const batch = await readBatch();
            if (!batch.length) break;
            entries.push(...batch);
          }
          const results = [];
          for (const child of entries) {
            results.push(...(await readEntry(child, path + entry.name + '/')));
          }
          resolve(results);
        } catch (err) {
          reject(err);
        }
      })();
    } else {
      resolve([]);
    }
  });
}

/**
 * 归一化入参为 [{ path, file }]：
 * - DataTransferItemList / DataTransferItem[]：用 webkitGetAsEntry 递归遍历（支持拖拽文件夹）
 * - [{ path, file }]：直接使用（文件夹选择/重试等场景）
 * - FileList / File[]：不支持 webkitGetAsEntry 时降级为普通文件；webkitdirectory 输入用 webkitRelativePath
 */
async function collectFiles(input) {
  if (!input || !input.length) return [];
  const first = input[0];

  // 1. DataTransferItemList / DataTransferItem[]
  if (first && typeof first.webkitGetAsEntry === 'function') {
    const results = [];
    for (const item of input) {
      const entry =
        typeof item.webkitGetAsEntry === 'function' ? item.webkitGetAsEntry() : null;
      if (entry) {
        results.push(...(await readEntry(entry)));
      } else {
        const file = typeof item.getAsFile === 'function' ? item.getAsFile() : null;
        if (file) results.push({ file, path: file.webkitRelativePath || file.name });
      }
    }
    return results;
  }

  // 2. [{ path, file }]
  if (first && first.file && typeof first.path === 'string') {
    return Array.from(input).map(({ path, file }) => ({ file, path }));
  }

  // 3. FileList / File[] / 无 webkitGetAsEntry 的 DataTransferItem（降级为普通文件）
  return Array.from(input)
    .map((item) => {
      const file =
        item && typeof item.getAsFile === 'function' ? item.getAsFile() : item;
      if (!file) return null;
      return { file, path: file.webkitRelativePath || file.name };
    })
    .filter(Boolean);
}

export function createUploader({
  basePath,
  onProgress,
  onComplete,
  onError,
  onAllComplete,
  concurrency = 5,
}) {
  const queue = [];
  const uploads = new Map(); // id -> item
  const stats = { success: 0, failed: 0, total: 0 };
  let active = 0;
  let idCounter = 0;
  let aborted = false;

  function generateId() {
    idCounter += 1;
    return `upload-${idCounter}-${Date.now()}`;
  }

  function makeItem(file, path, targetDir) {
    const item = {
      id: generateId(),
      file,
      path,
      name: path || file.name,
      targetDir,
      progress: 0,
      loaded: 0,
      total: file.size || 0,
      speed: 0,
      startTime: null,
      lastUptime: null,
      status: 'pending',
      error: null,
      xhr: null,
    };
    uploads.set(item.id, item);
    stats.total += 1;
    return item;
  }

  /**
   * 入队上传。files 支持 FileList / File[] / [{path, file}] / DataTransferItemList
   * （目录会通过 webkitGetAsEntry 递归展开，路径带相对路径信息）。
   * 返回 Promise<item[]>，供调用方拿到带 id 的上传项。
   */
  function enqueue(files, targetDir) {
    return collectFiles(files).then((entries) => {
      aborted = false;
      const created = [];
      for (const { file, path } of entries) {
        const item = makeItem(file, path, targetDir);
        queue.push(item);
        created.push(item);
      }
      processQueue();
      return created;
    });
  }

  function processQueue() {
    while (active < concurrency && queue.length > 0) {
      const item = queue.shift();
      active += 1;
      uploadItem(item);
    }
  }

  function uploadItem(item) {
    item.status = 'uploading';
    item.startTime = Date.now();
    item.lastUptime = item.startTime;

    const dir = (item.targetDir || '').replace(/\/+$/, '');
    const url =
      basePath + dir + '/' + encodeURIComponent(item.path).replace(/%2F/g, '/');
    const xhr = new XMLHttpRequest();

    xhr.upload.addEventListener('progress', (e) => {
      if (!e.lengthComputable || e.total <= 0) return;
      const now = Date.now();
      const dt = Math.max(now - item.lastUptime, 1);
      item.speed = ((e.loaded - item.loaded) / dt) * 1000; // bytes/s
      item.loaded = e.loaded;
      item.total = e.total;
      item.lastUptime = now;
      item.progress = Math.round((e.loaded / e.total) * 100);
      onProgress?.(item);
    });

    xhr.addEventListener('load', () => {
      active = Math.max(0, active - 1);
      if (xhr.status >= 200 && xhr.status < 300) {
        item.status = 'complete';
        item.progress = 100;
        stats.success += 1;
        onComplete?.(item);
      } else {
        item.status = 'failed';
        item.error = `HTTP ${xhr.status}`;
        stats.failed += 1;
        onError?.(item);
      }
      processQueue();
      checkAllDone();
    });

    xhr.addEventListener('error', () => {
      active = Math.max(0, active - 1);
      item.status = 'failed';
      item.error = 'Network error';
      stats.failed += 1;
      onError?.(item);
      processQueue();
      checkAllDone();
    });

    xhr.addEventListener('abort', () => {
      active = Math.max(0, active - 1);
      item.status = 'cancelled';
      processQueue();
      checkAllDone();
    });

    xhr.open('PUT', url);
    const token = getToken();
    if (token) xhr.setRequestHeader('Authorization', `Bearer ${token}`);
    xhr.send(item.file);
    item.xhr = xhr;
  }

  // 全部任务结束后回调 onAllComplete({ success, failed, total })
  function checkAllDone() {
    if (aborted) return;
    if (active === 0 && queue.length === 0 && uploads.size > 0) {
      onAllComplete?.({
        success: stats.success,
        failed: stats.failed,
        total: stats.total,
      });
    }
  }

  // 取消单个上传项（标记 cancelled，保留在记录中）
  function cancel(id) {
    const item = uploads.get(id);
    if (!item) return;
    if (item.xhr && item.status === 'uploading') item.xhr.abort();
    item.status = 'cancelled';
    const idx = queue.findIndex((i) => i.id === id);
    if (idx >= 0) queue.splice(idx, 1);
    processQueue();
    checkAllDone();
  }

  // 硬清除：中止所有 XHR 并清空记录
  function cancelAll() {
    aborted = true;
    for (const item of uploads.values()) {
      if (item.xhr) item.xhr.abort();
    }
    uploads.clear();
    queue.length = 0;
    active = 0;
  }

  // 取消所有进行中的上传（保留记录并标记 cancelled，不触发 onAllComplete）
  function abortAll() {
    aborted = true;
    for (const item of uploads.values()) {
      if (item.xhr && item.status === 'uploading') item.xhr.abort();
    }
    for (const item of queue) item.status = 'cancelled';
    queue.length = 0;
    checkAllDone();
  }

  return {
    enqueue,
    cancel,
    cancelAll,
    abortAll,
    getUploads: () => [...uploads.values()],
  };
}
