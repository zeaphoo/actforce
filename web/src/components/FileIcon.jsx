/**
 * 文件类型图标组件
 * 用于根据文件后缀名显示对应的 PNG 图标
 */
function FileIcon({ filename, className = "w-8 h-8 object-contain", alt = "" }) {
  // 获取文件后缀名
  const getExtension = (name) => {
    if (!name) return 'default';
    const parts = name.split('.');
    if (parts.length <= 1) return 'default';
    const ext = parts.pop().toLowerCase();
    // 映射旧版 Office 格式到新版图标
    const map = {
      'doc': 'docx',
      'xls': 'xlsx',
      'ppt': 'pptx'
    };
    return map[ext] || ext;
  };

  const extension = getExtension(filename);
  
  // 目前支持的图标映射（根据 public/file-icons 目录下的文件命名）
  // 如果对应的 png 不存在，<img> 的 onError 会触发显示默认图标
  const iconPath = `/file-icons/${extension}.png`;
  const defaultPath = '/file-icons/default.png';

  return (
    <img 
      src={iconPath} 
      className={className} 
      alt={alt || extension}
      title={alt || extension}
      onError={(e) => {
        if (e.target.src !== window.location.origin + defaultPath) {
          e.target.src = defaultPath;
        }
      }}
    />
  );
}

export default FileIcon;
