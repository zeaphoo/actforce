import React, { useState, useRef, useEffect, useCallback, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Play,
  Pause,
  SkipBack,
  SkipForward,
  Shuffle,
  Repeat,
  Repeat1,
  Volume2,
  VolumeX,
  X,
  Music,
  ChevronLeft,
  ChevronRight,
} from 'lucide-react';
import { cn, extname } from '../lib/utils.js';
import { getCommonMetadata } from '../lib/mami-chan/index.js';

// 与参照实现一致的音频扩展名集合（含 actforce AUDIO_EXTS 的 wav/aac）
const AUDIO_EXTS = new Set(['mp3', 'm4a', 'ogg', 'weba', 'oga', 'flac', 'opus', 'wav', 'aac']);

function formatAudioTime(seconds) {
  if (isNaN(seconds) || seconds < 0 || !isFinite(seconds)) return '0:00';
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins}:${secs.toString().padStart(2, '0')}`;
}

// 从 file 对象中解析出可用的音频 URL（兼容 url / fullpath / previewUrl / src 等字段）
function fileUrl(f) {
  if (!f) return '';
  if (typeof f === 'string') return f;
  return f.url || f.fullpath || f.previewUrl || f.src || f.href || f.path || '';
}

function fileName(f) {
  if (!f) return '';
  if (typeof f === 'string') return f.split('/').filter(Boolean).pop() || '';
  return f.name || f.filename || '';
}

function fileExt(f) {
  if (!f) return '';
  if (f.ext) return String(f.ext).toLowerCase();
  return extname(fileName(f));
}

function isAudioFile(f) {
  return !!f && !f.is_dir && AUDIO_EXTS.has(fileExt(f));
}

// 判断两个 file 对象是否指向同一音轨（按 URL 或文件名）
function sameFile(a, b) {
  if (!a || !b) return false;
  const ua = fileUrl(a);
  const ub = fileUrl(b);
  if (ua && ub) return ua === ub;
  const na = fileName(a);
  const nb = fileName(b);
  return !!na && na === nb;
}

function stripExt(name) {
  if (!name) return '';
  return name.replace(/\.[^/.]+$/, '');
}

/**
 * 触摸友好的滑块（同时支持点击、拖动、键盘）
 * Props: { value, max, step, onChange, onCommit, className, ariaLabel }
 */
function Slider({ value, max = 100, step = 1, onChange, onCommit, className, ariaLabel }) {
  const trackRef = useRef(null);
  const [dragging, setDragging] = useState(false);

  const valueFromClientX = useCallback((clientX) => {
    const el = trackRef.current;
    if (!el || !max) return 0;
    const rect = el.getBoundingClientRect();
    const ratio = rect.width > 0 ? (clientX - rect.left) / rect.width : 0;
    const raw = Math.min(max, Math.max(0, ratio * max));
    return step > 0 ? Math.round(raw / step) * step : raw;
  }, [max, step]);

  const handlePointerDown = (e) => {
    e.preventDefault();
    setDragging(true);
    try { e.currentTarget.setPointerCapture(e.pointerId); } catch { /* noop */ }
    onChange?.(valueFromClientX(e.clientX));
  };

  const handlePointerMove = (e) => {
    if (!dragging) return;
    onChange?.(valueFromClientX(e.clientX));
  };

  const handlePointerUp = (e) => {
    if (!dragging) return;
    setDragging(false);
    try { e.currentTarget.releasePointerCapture(e.pointerId); } catch { /* noop */ }
    onCommit?.(valueFromClientX(e.clientX));
  };

  const percent = max > 0 ? Math.min(100, Math.max(0, (value / max) * 100)) : 0;

  return (
    <div
      ref={trackRef}
      role="slider"
      aria-label={ariaLabel}
      aria-valuemin={0}
      aria-valuemax={max}
      aria-valuenow={value}
      tabIndex={0}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={() => setDragging(false)}
      onKeyDown={(e) => {
        if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
          e.preventDefault();
          onChange?.(Math.min(max, value + step));
        } else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
          e.preventDefault();
          onChange?.(Math.max(0, value - step));
        } else if (e.key === 'Home') {
          e.preventDefault();
          onChange?.(0);
        } else if (e.key === 'End') {
          e.preventDefault();
          onChange?.(max);
        }
      }}
      className={cn(
        'relative flex items-center cursor-pointer touch-none select-none outline-none h-5',
        'focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:ring-offset-1 focus-visible:ring-offset-white dark:focus-visible:ring-offset-gray-900 rounded',
        className
      )}
    >
      <div className="relative h-1.5 w-full rounded-full bg-gray-200 dark:bg-gray-700 pointer-events-none">
        <div
          className="absolute inset-y-0 left-0 rounded-full bg-blue-500 pointer-events-none"
          style={{ width: `${percent}%` }}
        />
      </div>
      <div
        className="pointer-events-none absolute h-4 w-4 rounded-full bg-blue-500 border-2 border-white dark:border-gray-900 shadow"
        style={{ left: `${percent}%`, transform: 'translateX(-50%)' }}
      />
    </div>
  );
}

/**
 * 音频播放器组件
 * - 参照（dufs-astro-assets）风格：<AudioPlayer file={file} files={paths} onClose={...} />
 *   → 固定在底部的完整播放条（封面、播放列表、随机、循环、音量、进度、MediaSession）
 * - 兼容原 actforce 风格：<AudioPlayer src={url} filename={name} onReady={...} onError={...} minimal />
 *   → 内联卡片形态（FilePreview 用法），同样支持封面 / 元数据 / 音量 / 循环 / MediaSession
 */
export function AudioPlayer({ file, files, onClose, src, filename, onReady, onError, minimal, autoPlay = true }) {
  const { t } = useTranslation();
  const audioRef = useRef(null);
  const coverUrlRef = useRef(null);
  const onReadyRef = useRef(onReady);
  const onErrorRef = useRef(onError);
  const lastPropFileRef = useRef(null);

  // 当前音轨（内部状态，支持播放列表内 prev/next 切换）
  const [currentFile, setCurrentFile] = useState(() => (
    file || (src ? { url: src, name: filename || '', ext: '' } : null)
  ));

  // 播放器状态
  const [isPaused, setIsPaused] = useState(true);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [isMuted, setIsMuted] = useState(false);
  const [shuffle, setShuffle] = useState(false);
  const [repeatMode, setRepeatMode] = useState('off'); // 'off' | 'on' | 'once'

  // 元数据
  const [metadata, setMetadata] = useState({ title: '', artist: '', album: '', cover: null });

  useEffect(() => { onReadyRef.current = onReady; }, [onReady]);
  useEffect(() => { onErrorRef.current = onError; }, [onError]);

  // 外部 props 显式切换目标音轨时跟随；内部 prev/next 导航不受父组件无关重渲染影响
  useEffect(() => {
    const propFile = file || (src ? { url: src, name: filename || '', ext: '' } : null);
    if (!propFile) return;
    if (!sameFile(lastPropFileRef.current, propFile)) {
      setCurrentFile(propFile);
    }
    lastPropFileRef.current = propFile;
  }, [file, src, filename]);

  // 播放列表：files 过滤出音频；否则单曲模式
  const playlist = useMemo(() => {
    if (Array.isArray(files) && files.length > 0) {
      return files.filter(isAudioFile);
    }
    return currentFile ? [currentFile] : [];
  }, [files, currentFile]);

  const currentIndex = useMemo(() => {
    const idx = playlist.findIndex((f) => sameFile(f, currentFile));
    return idx >= 0 ? idx : 0;
  }, [playlist, currentFile]);

  // 加载元数据（失败静默降级，不阻塞播放）
  useEffect(() => {
    if (!currentFile) return;
    const url = fileUrl(currentFile);
    if (!url) return;

    let cancelled = false;

    async function loadMetadata() {
      try {
        const data = await getCommonMetadata(url);
        if (cancelled) return;
        // 释放上一张封面的 blob URL
        if (coverUrlRef.current && coverUrlRef.current !== data.cover && coverUrlRef.current.startsWith('blob:')) {
          try { URL.revokeObjectURL(coverUrlRef.current); } catch { /* noop */ }
        }
        coverUrlRef.current = data.cover || null;
        setMetadata({
          title: data.title || '',
          artist: data.artist || '',
          album: data.album || '',
          cover: data.cover || null,
        });
      } catch (error) {
        if (cancelled) return;
        console.warn('Failed to load audio metadata:', error);
        setMetadata({ title: '', artist: '', album: '', cover: null });
      }
    }

    loadMetadata();

    return () => { cancelled = true; };
  }, [currentFile]);

  // 组件卸载时释放封面 blob URL
  useEffect(() => {
    return () => {
      if (coverUrlRef.current && coverUrlRef.current.startsWith('blob:')) {
        try { URL.revokeObjectURL(coverUrlRef.current); } catch { /* noop */ }
        coverUrlRef.current = null;
      }
    };
  }, []);

  // 上一首 / 下一首
  const playPrevious = useCallback(() => {
    if (playlist.length <= 1) return;
    let newIndex;
    if (shuffle) {
      newIndex = Math.floor(Math.random() * playlist.length);
      if (newIndex === currentIndex && playlist.length > 1) {
        newIndex = (currentIndex - 1 + playlist.length) % playlist.length;
      }
    } else {
      newIndex = (currentIndex - 1 + playlist.length) % playlist.length;
    }
    setCurrentFile(playlist[newIndex]);
    setIsPaused(false);
  }, [playlist, currentIndex, shuffle]);

  const playNext = useCallback(() => {
    if (playlist.length <= 1) return;
    let newIndex;
    if (shuffle) {
      newIndex = Math.floor(Math.random() * playlist.length);
      if (newIndex === currentIndex && playlist.length > 1) {
        newIndex = (currentIndex + 1) % playlist.length;
      }
    } else {
      newIndex = (currentIndex + 1) % playlist.length;
    }
    setCurrentFile(playlist[newIndex]);
    setIsPaused(false);
  }, [playlist, currentIndex, shuffle]);

  // 播放 / 暂停
  const togglePlay = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;
    if (isPaused) {
      audio.play().catch(() => {});
    } else {
      audio.pause();
    }
  }, [isPaused]);

  // 循环模式三态：off → on（列表循环）→ once（单曲循环）
  const toggleRepeat = useCallback(() => {
    setRepeatMode((prev) => (prev === 'off' ? 'on' : prev === 'on' ? 'once' : 'off'));
  }, []);

  const toggleMute = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;
    audio.muted = !isMuted;
    setIsMuted(!isMuted);
  }, [isMuted]);

  const handleSeek = useCallback((value) => {
    const audio = audioRef.current;
    if (!audio) return;
    audio.currentTime = value;
    setCurrentTime(value);
  }, []);

  const handleVolumeChange = useCallback((value) => {
    const audio = audioRef.current;
    if (!audio) return;
    audio.volume = value;
    setVolume(value);
    if (value > 0 && isMuted) {
      audio.muted = false;
      setIsMuted(false);
    }
  }, [isMuted]);

  // MediaSession 集成（切歌时更新）
  useEffect(() => {
    if (!('mediaSession' in navigator)) return;

    const title = metadata.title || stripExt(fileName(currentFile)) || t('audioUnknownTitle', { defaultValue: '未知音频' });
    navigator.mediaSession.metadata = new MediaMetadata({
      title,
      artist: metadata.artist || '',
      album: metadata.album || '',
      artwork: metadata.cover
        ? [{ src: metadata.cover, sizes: '512x512', type: 'image/jpeg' }]
        : [],
    });

    navigator.mediaSession.setActionHandler('play', () => {
      audioRef.current?.play().catch(() => {});
    });
    navigator.mediaSession.setActionHandler('pause', () => {
      audioRef.current?.pause();
    });
    navigator.mediaSession.setActionHandler('previoustrack', playPrevious);
    navigator.mediaSession.setActionHandler('nexttrack', playNext);
    navigator.mediaSession.setActionHandler('seekto', (d) => {
      if (audioRef.current && d.seekTime != null) {
        audioRef.current.currentTime = d.seekTime;
      }
    });

    return () => {
      navigator.mediaSession.setActionHandler('play', null);
      navigator.mediaSession.setActionHandler('pause', null);
      navigator.mediaSession.setActionHandler('previoustrack', null);
      navigator.mediaSession.setActionHandler('nexttrack', null);
      navigator.mediaSession.setActionHandler('seekto', null);
    };
  }, [metadata, currentFile, playPrevious, playNext, t]);

  // 音频事件监听
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;

    const handlePlay = () => {
      setIsPaused(false);
      if ('mediaSession' in navigator) navigator.mediaSession.playbackState = 'playing';
    };
    const handlePause = () => {
      setIsPaused(true);
      if ('mediaSession' in navigator) navigator.mediaSession.playbackState = 'paused';
    };
    const handleTimeUpdate = () => setCurrentTime(audio.currentTime);
    const handleLoadedMetadata = () => {
      setDuration(audio.duration || 0);
      onReadyRef.current?.();
    };
    const handleEnded = () => {
      if (repeatMode === 'once') {
        audio.currentTime = 0;
        audio.play().catch(() => {});
      } else if (repeatMode === 'on' || playlist.length > 1) {
        playNext();
      } else {
        setIsPaused(true);
      }
    };
    const handleError = () => {
      onErrorRef.current?.(new Error('Audio failed'));
      setIsPaused(true);
    };

    audio.addEventListener('play', handlePlay);
    audio.addEventListener('pause', handlePause);
    audio.addEventListener('timeupdate', handleTimeUpdate);
    audio.addEventListener('loadedmetadata', handleLoadedMetadata);
    audio.addEventListener('ended', handleEnded);
    audio.addEventListener('error', handleError);

    return () => {
      audio.removeEventListener('play', handlePlay);
      audio.removeEventListener('pause', handlePause);
      audio.removeEventListener('timeupdate', handleTimeUpdate);
      audio.removeEventListener('loadedmetadata', handleLoadedMetadata);
      audio.removeEventListener('ended', handleEnded);
      audio.removeEventListener('error', handleError);
    };
  }, [repeatMode, playlist.length, playNext, currentFile]);

  // 切歌时自动加载并播放（自动播放可能被浏览器阻止，静默降级）
  useEffect(() => {
    const audio = audioRef.current;
    if (!currentFile || !audio || !autoPlay) return;
    audio.load();
    audio.play().catch(() => setIsPaused(true));
  }, [currentFile, autoPlay]);

  if (!currentFile) return null;

  const displayTitle = metadata.title || stripExt(fileName(currentFile)) || t('audioUnknownTitle', { defaultValue: '未知音频' });
  const displayArtist = metadata.artist || '';
  const displayAlbum = metadata.album || '';

  const RepeatIcon = repeatMode === 'once' ? Repeat1 : Repeat;
  const repeatActive = repeatMode !== 'off';

  const floating = !!file || (Array.isArray(files) && files.length > 0);

  // ---------- 浮动播放条（参照形态：file / files / onClose） ----------
  if (floating) {
    return (
      <div className="fixed bottom-0 left-0 right-0 z-50 bg-white dark:bg-gray-900 border-t border-gray-200 dark:border-gray-700 shadow-lg">
        <audio ref={audioRef} src={fileUrl(currentFile)} preload="metadata" loop={false} />

        <div className="max-w-7xl mx-auto px-4 py-3">
          <div className="flex items-center gap-3 sm:gap-4">
            {/* 封面图 */}
            <div className="shrink-0 w-12 h-12 sm:w-16 sm:h-16 rounded-md overflow-hidden bg-gray-100 dark:bg-gray-800 flex items-center justify-center">
              {metadata.cover ? (
                <img
                  src={metadata.cover}
                  alt={t('audioCover', { defaultValue: '封面' })}
                  className="w-full h-full object-cover"
                />
              ) : (
                <Music className="h-6 w-6 sm:h-8 sm:w-8 text-gray-400" />
              )}
            </div>

            {/* 歌曲信息 */}
            <div className="flex-grow min-w-0 hidden sm:block">
              <div className="font-medium text-gray-800 dark:text-gray-200 truncate">{displayTitle}</div>
              {(displayArtist || displayAlbum) && (
                <div className="text-sm text-gray-500 dark:text-gray-400 truncate">
                  {displayArtist}
                  {displayAlbum && ` - ${displayAlbum}`}
                </div>
              )}
            </div>

            {/* 播放控制 */}
            <div className="flex items-center gap-1 sm:gap-2">
              {/* 随机播放 */}
              <button
                type="button"
                onClick={() => setShuffle(!shuffle)}
                title={t('audioShuffle', { defaultValue: '随机播放' })}
                className={cn(
                  'h-8 w-8 flex items-center justify-center rounded hover:bg-gray-100 dark:hover:bg-gray-800',
                  shuffle ? 'text-blue-500' : 'text-gray-400'
                )}
              >
                <Shuffle className="h-4 w-4" />
              </button>

              {/* 上一首 */}
              <button
                type="button"
                onClick={playPrevious}
                disabled={playlist.length <= 1}
                title={t('audioPrevious', { defaultValue: '上一首' })}
                className="h-8 w-8 flex items-center justify-center rounded text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800 disabled:opacity-30 disabled:hover:bg-transparent"
              >
                <SkipBack className="h-4 w-4" />
              </button>

              {/* 播放 / 暂停 */}
              <button
                type="button"
                onClick={togglePlay}
                title={isPaused ? t('audioPlay', { defaultValue: '播放' }) : t('audioPause', { defaultValue: '暂停' })}
                className="h-10 w-10 rounded-full bg-blue-500 text-white hover:bg-blue-600 flex items-center justify-center"
              >
                {isPaused ? <Play className="h-5 w-5 ml-0.5" /> : <Pause className="h-5 w-5" />}
              </button>

              {/* 下一首 */}
              <button
                type="button"
                onClick={playNext}
                disabled={playlist.length <= 1}
                title={t('audioNext', { defaultValue: '下一首' })}
                className="h-8 w-8 flex items-center justify-center rounded text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800 disabled:opacity-30 disabled:hover:bg-transparent"
              >
                <SkipForward className="h-4 w-4" />
              </button>

              {/* 循环模式 */}
              <button
                type="button"
                onClick={toggleRepeat}
                title={repeatMode === 'once' ? t('audioRepeatOne', { defaultValue: '单曲循环' }) : t('audioRepeat', { defaultValue: '循环播放' })}
                className={cn(
                  'h-8 w-8 flex items-center justify-center rounded hover:bg-gray-100 dark:hover:bg-gray-800',
                  repeatActive ? 'text-blue-500' : 'text-gray-400'
                )}
              >
                <RepeatIcon className="h-4 w-4" />
              </button>
            </div>

            {/* 进度条和时间（md+） */}
            <div className="flex-grow max-w-md hidden md:flex items-center gap-3">
              <span className="text-xs text-gray-500 dark:text-gray-400 w-10 text-right shrink-0">
                {formatAudioTime(currentTime)}
              </span>
              <Slider
                value={currentTime}
                max={duration || 100}
                step={1}
                onChange={handleSeek}
                className="flex-grow"
                ariaLabel={t('audioSeek', { defaultValue: '播放进度' })}
              />
              <span className="text-xs text-gray-500 dark:text-gray-400 w-10 shrink-0">
                {formatAudioTime(duration)}
              </span>
            </div>

            {/* 音量控制（sm+） */}
            <div className="hidden sm:flex items-center gap-2">
              <button
                type="button"
                onClick={toggleMute}
                title={isMuted || volume === 0 ? t('audioUnmute', { defaultValue: '取消静音' }) : t('audioMute', { defaultValue: '静音' })}
                className="h-8 w-8 flex items-center justify-center rounded text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800"
              >
                {isMuted || volume === 0 ? <VolumeX className="h-4 w-4" /> : <Volume2 className="h-4 w-4" />}
              </button>
              <Slider
                value={isMuted ? 0 : volume}
                max={1}
                step={0.01}
                onChange={handleVolumeChange}
                className="w-20"
                ariaLabel={t('audioVolume', { defaultValue: '音量' })}
              />
            </div>

            {/* 关闭 */}
            {onClose && (
              <button
                type="button"
                onClick={onClose}
                title={t('audioClose', { defaultValue: '关闭' })}
                className="h-8 w-8 flex items-center justify-center rounded text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800"
              >
                <X className="h-4 w-4" />
              </button>
            )}
          </div>

          {/* 移动端进度条（较大可点区域） */}
          <div className="flex md:hidden items-center gap-2 mt-2">
            <span className="text-xs text-gray-500 dark:text-gray-400 w-10 text-right shrink-0">
              {formatAudioTime(currentTime)}
            </span>
            <Slider
              value={currentTime}
              max={duration || 100}
              step={1}
              onChange={handleSeek}
              className="flex-grow h-6"
              ariaLabel={t('audioSeek', { defaultValue: '播放进度' })}
            />
            <span className="text-xs text-gray-500 dark:text-gray-400 w-10 shrink-0">
              {formatAudioTime(duration)}
            </span>
          </div>
        </div>
      </div>
    );
  }

  // ---------- 内联卡片形态（兼容原 actforce：src / filename / onReady / onError / minimal） ----------
  return (
    <div className={cn('w-full bg-gray-50 dark:bg-gray-800 rounded-lg', minimal ? 'p-3' : 'p-4')}>
      <audio ref={audioRef} src={fileUrl(currentFile)} preload="metadata" loop={false} />

      {!minimal && (
        <div className="flex items-center gap-3 mb-3">
          <div className="shrink-0 w-12 h-12 rounded-md overflow-hidden bg-gray-200 dark:bg-gray-700 flex items-center justify-center">
            {metadata.cover ? (
              <img
                src={metadata.cover}
                alt={t('audioCover', { defaultValue: '封面' })}
                className="w-full h-full object-cover"
              />
            ) : (
              <Music className="h-6 w-6 text-gray-400" />
            )}
          </div>
          <div className="flex-1 min-w-0">
            <p className="text-sm font-medium text-gray-800 dark:text-gray-200 truncate">{displayTitle}</p>
            {(displayArtist || displayAlbum) && (
              <p className="text-xs text-gray-500 dark:text-gray-400 truncate">
                {displayArtist}
                {displayAlbum && ` - ${displayAlbum}`}
              </p>
            )}
          </div>
        </div>
      )}

      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={togglePlay}
          title={isPaused ? t('audioPlay', { defaultValue: '播放' }) : t('audioPause', { defaultValue: '暂停' })}
          className="shrink-0 w-10 h-10 flex items-center justify-center rounded-full bg-blue-500 text-white hover:bg-blue-600"
        >
          {isPaused ? <Play size={18} className="ml-0.5" /> : <Pause size={18} />}
        </button>

        <div className="flex-1 min-w-0">
          <Slider
            value={currentTime}
            max={duration || 100}
            step={1}
            onChange={handleSeek}
            ariaLabel={t('audioSeek', { defaultValue: '播放进度' })}
          />
          <div className="flex justify-between mt-1 text-xs text-gray-500 dark:text-gray-400">
            <span>{formatAudioTime(currentTime)}</span>
            <span>{formatAudioTime(duration)}</span>
          </div>
        </div>

        <button
          type="button"
          onClick={toggleRepeat}
          title={repeatMode === 'once' ? t('audioRepeatOne', { defaultValue: '单曲循环' }) : t('audioRepeat', { defaultValue: '循环播放' })}
          className={cn(
            'shrink-0 h-8 w-8 flex items-center justify-center rounded hover:bg-gray-200 dark:hover:bg-gray-700',
            repeatActive ? 'text-blue-500' : 'text-gray-400'
          )}
        >
          <RepeatIcon size={16} />
        </button>

        <button
          type="button"
          onClick={toggleMute}
          title={isMuted || volume === 0 ? t('audioUnmute', { defaultValue: '取消静音' }) : t('audioMute', { defaultValue: '静音' })}
          className="shrink-0 h-8 w-8 flex items-center justify-center rounded text-gray-400 hover:bg-gray-200 dark:hover:bg-gray-700"
        >
          {isMuted || volume === 0 ? <VolumeX size={16} /> : <Volume2 size={16} />}
        </button>

        {!minimal && (
          <div className="hidden sm:flex items-center shrink-0">
            <Slider
              value={isMuted ? 0 : volume}
              max={1}
              step={0.01}
              onChange={handleVolumeChange}
              className="w-16"
              ariaLabel={t('audioVolume', { defaultValue: '音量' })}
            />
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * 兼容旧的底部浮动条 API：playlist / currentIndex / onPlay(index) / onClose
 * （播放列表状态由父组件管理，内部复用增强后的 AudioPlayer 单曲形态）
 */
export function AudioPlayerBar({ playlist, currentIndex, onPlay, onClose }) {
  if (!playlist || playlist.length === 0) return null;

  const entry = playlist[currentIndex];
  if (!entry) return null;

  return (
    <div className="fixed bottom-0 left-0 right-0 z-40 bg-white dark:bg-gray-900 border-t border-gray-200 dark:border-gray-700 shadow-lg">
      <div className="max-w-4xl mx-auto flex items-center gap-3 px-4 py-2">
        <button
          type="button"
          onClick={() => onPlay?.(currentIndex - 1)}
          disabled={currentIndex <= 0}
          title="上一首"
          className="p-1 text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 disabled:opacity-30 disabled:hover:text-gray-500 dark:disabled:hover:text-gray-400"
        >
          <ChevronLeft size={16} />
        </button>
        <button
          type="button"
          onClick={() => onPlay?.(currentIndex + 1)}
          disabled={currentIndex >= playlist.length - 1}
          title="下一首"
          className="p-1 text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 disabled:opacity-30 disabled:hover:text-gray-500 dark:disabled:hover:text-gray-400"
        >
          <ChevronRight size={16} />
        </button>
        <div className="flex-1 min-w-0">
          <AudioPlayer
            src={entry.url || entry.previewUrl || entry.fullpath}
            filename={entry.name}
            minimal
          />
        </div>
        <span className="text-xs text-gray-500 dark:text-gray-400 shrink-0 hidden sm:block max-w-[8rem] truncate">
          {entry.name}
        </span>
        <button
          type="button"
          onClick={onClose}
          title="关闭"
          className="p-1 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200"
        >
          <X size={16} />
        </button>
      </div>
    </div>
  );
}
