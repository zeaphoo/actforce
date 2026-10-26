// Components
export { AppBar } from './components/AppBar.jsx';
export { Footer } from './components/Footer.jsx';
export { FileBrowser } from './components/FileBrowser.jsx';
export { FileBreadcrumb } from './components/FileBreadcrumb.jsx';
export { FileUploader } from './components/FileUploader.jsx';
export { FilePreview } from './components/FilePreview.jsx';
export { FileActions } from './components/FileActions.jsx';
export { SearchBar } from './components/SearchBar.jsx';
export { AudioPlayer, AudioPlayerBar } from './components/AudioPlayer.jsx';
export { ReadmeCard } from './components/ReadmeCard.jsx';
export { EditDialog } from './components/EditDialog.jsx';
export { CreateDialog } from './components/CreateDialog.jsx';
export { OfficePreview } from './components/OfficePreview.jsx';
export { MarkdownRenderer } from './components/MarkdownRenderer.jsx';
export { ToastProvider } from './components/ToastProvider.jsx';
export { DialogProvider } from './components/DialogProvider.jsx';

// shadcn/ui 组件（自 dufs-astro-assets 参考实现复制，packages/file-manager/components/ui）
export { Badge, badgeVariants } from './components/ui/badge.jsx';
export { Button, buttonVariants } from './components/ui/button.jsx';
export {
  Card, CardHeader, CardFooter, CardTitle, CardAction, CardDescription, CardContent,
} from './components/ui/card.jsx';
export {
  Dialog, DialogClose, DialogContent, DialogDescription, DialogFooter, DialogHeader,
  DialogOverlay, DialogPortal, DialogTitle, DialogTrigger,
} from './components/ui/dialog.jsx';
export {
  DropdownMenu, DropdownMenuPortal, DropdownMenuTrigger, DropdownMenuContent,
  DropdownMenuGroup, DropdownMenuLabel, DropdownMenuItem, DropdownMenuCheckboxItem,
  DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuSeparator,
  DropdownMenuShortcut, DropdownMenuSub, DropdownMenuSubTrigger, DropdownMenuSubContent,
} from './components/ui/dropdown-menu.jsx';
export { Input } from './components/ui/input.jsx';
export {
  Popover, PopoverTrigger, PopoverContent, PopoverAnchor,
  PopoverHeader, PopoverTitle, PopoverDescription,
} from './components/ui/popover.jsx';
export { Progress } from './components/ui/progress.jsx';
export { ScrollArea, ScrollBar } from './components/ui/scroll-area.jsx';
export {
  Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectScrollDownButton,
  SelectScrollUpButton, SelectSeparator, SelectTrigger, SelectValue,
} from './components/ui/select.jsx';
export { Separator } from './components/ui/separator.jsx';
export { Slider } from './components/ui/slider.jsx';
export { Switch } from './components/ui/switch.jsx';
export {
  Table, TableHeader, TableBody, TableFooter, TableHead, TableRow, TableCell, TableCaption,
} from './components/ui/table.jsx';
export { Tooltip, TooltipTrigger, TooltipContent, TooltipProvider } from './components/ui/tooltip.jsx';

// Hooks & Store
export { useFileService } from './hooks/useFileService.js';
export { useFileStore } from './lib/fileStore.js';

// Lib
export { getToken, authHeaders, apiFetch, apiJson } from './lib/api.js';
export {
  formatSize, formatDate, debounce, cn,
  extname, basename, dirname,
  isTextFile, fileCategory, mimeType,
} from './lib/utils.js';
export { default as i18n } from './lib/i18n.js';
export { createUploader } from './lib/uploader.js';
export { toast } from './lib/toast.js';
export { dialog } from './lib/dialog.js';
