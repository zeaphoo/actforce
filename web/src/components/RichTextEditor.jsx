import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { EditorContent, useEditor } from '@tiptap/react';
import { Extension } from '@tiptap/core';
import { StarterKit } from '@tiptap/starter-kit';
import { BulletList } from '@tiptap/extension-bullet-list';
import { OrderedList } from '@tiptap/extension-ordered-list';
import { ListItem } from '@tiptap/extension-list-item';
import { Blockquote } from '@tiptap/extension-blockquote';
import { CodeBlock } from '@tiptap/extension-code-block';
import { TextAlign } from '@tiptap/extension-text-align';
import { Highlight } from '@tiptap/extension-highlight';
import { TextStyle } from '@tiptap/extension-text-style';
import { Color } from '@tiptap/extension-color';
import { Placeholder } from '@tiptap/extension-placeholder';
import { Bold, Italic, Strikethrough, List, ListOrdered, Quote, Code, AlignLeft, AlignCenter, AlignRight, Paintbrush, Highlighter, Eraser, Ban, ZoomIn, ZoomOut, RotateCcw, ArrowDownToLine, ArrowUpFromLine } from 'lucide-react';

const jsonStringifySafe = (v) => {
  try {
    return JSON.stringify(v);
  } catch {
    return '';
  }
};

const FontSize = Extension.create({
  name: 'fontSize',
  addGlobalAttributes() {
    return [
      {
        types: ['textStyle'],
        attributes: {
          fontSize: {
            default: null,
            parseHTML: (element) => {
              const v = element.style?.fontSize;
              return v || null;
            },
            renderHTML: (attributes) => {
              if (!attributes.fontSize) return {};
              return { style: `font-size: ${attributes.fontSize}` };
            },
          },
        },
      },
    ];
  },
  addCommands() {
    return {
      setFontSize:
        (fontSize) =>
        ({ chain }) => {
          return chain().setMark('textStyle', { fontSize }).run();
        },
      unsetFontSize:
        () =>
        ({ chain }) => {
          return chain().setMark('textStyle', { fontSize: null }).removeEmptyTextStyle().run();
        },
    };
  },
});

const LineHeight = Extension.create({
  name: 'lineHeight',
  addGlobalAttributes() {
    return [
      {
        types: ['paragraph', 'heading'],
        attributes: {
          lineHeight: {
            default: null,
            parseHTML: (element) => {
              const v = element.style?.lineHeight;
              return v || null;
            },
            renderHTML: (attributes) => {
              if (!attributes.lineHeight) return {};
              return { style: `line-height: ${attributes.lineHeight}` };
            },
          },
        },
      },
    ];
  },
  addCommands() {
    return {
      setLineHeight:
        (lineHeight) =>
        ({ editor }) => {
          const { tr, doc, selection } = editor.state;
          const { from, to } = selection;
          const types = new Set(['paragraph', 'heading']);
          let changed = false;
          doc.nodesBetween(from, to, (node, pos) => {
            if (!types.has(node.type.name)) return;
            tr.setNodeMarkup(pos, undefined, { ...node.attrs, lineHeight });
            changed = true;
          });
          if (!changed) {
            const parent = selection.$from.parent;
            const depth = selection.$from.depth;
            const pos = selection.$from.before(depth);
            if (types.has(parent.type.name)) {
              tr.setNodeMarkup(pos, undefined, { ...parent.attrs, lineHeight });
              changed = true;
            }
          }
          if (changed) {
            editor.view.dispatch(tr);
          }
          return changed;
        },
      unsetLineHeight:
        () =>
        ({ editor }) => {
          const { tr, doc, selection } = editor.state;
          const { from, to } = selection;
          const types = new Set(['paragraph', 'heading']);
          let changed = false;
          doc.nodesBetween(from, to, (node, pos) => {
            if (!types.has(node.type.name)) return;
            if (!node.attrs?.lineHeight) return;
            const nextAttrs = { ...node.attrs };
            delete nextAttrs.lineHeight;
            tr.setNodeMarkup(pos, undefined, nextAttrs);
            changed = true;
          });
          if (!changed) {
            const parent = selection.$from.parent;
            const depth = selection.$from.depth;
            const pos = selection.$from.before(depth);
            if (types.has(parent.type.name) && parent.attrs?.lineHeight) {
              const nextAttrs = { ...parent.attrs };
              delete nextAttrs.lineHeight;
              tr.setNodeMarkup(pos, undefined, nextAttrs);
              changed = true;
            }
          }
          if (changed) {
            editor.view.dispatch(tr);
          }
          return changed;
        },
    };
  },
});

const RichTextEditor = ({
  content = '',
  contentJson = null,
  onChange,
  placeholder = '请输入内容...',
  className = '',
}) => {
  const [textColor, setTextColor] = useState('#111827');
  const [bgColor, setBgColor] = useState('#fff59d');

  const preventMouseDown = useCallback((e) => {
    e.preventDefault();
  }, []);

  const runOnMouseDown = useCallback((fn) => {
    return (e) => {
      e.preventDefault();
      fn?.();
    };
  }, []);

  const extensions = useMemo(() => {
    return [
      StarterKit.configure({
        bulletList: false,
        orderedList: false,
        listItem: false,
        blockquote: false,
        codeBlock: false,
      }),
      BulletList,
      OrderedList,
      ListItem,
      Blockquote,
      CodeBlock,
      TextStyle,
      Color,
      FontSize,
      LineHeight,
      Highlight.configure({ multicolor: true }),
      TextAlign.configure({
        types: ['heading', 'paragraph'],
      }),
      Placeholder.configure({
        placeholder,
      }),
    ];
  }, [placeholder]);

  const clampFontPx = (n) => {
    if (!Number.isFinite(n)) return 16;
    return Math.max(12, Math.min(32, n));
  };

  const clampLineHeight = (n) => {
    if (!Number.isFinite(n)) return 1.6;
    return Math.max(1.0, Math.min(2.5, n));
  };

  const getCurrentFontPx = () => {
    try {
      const fs = editor?.getAttributes('textStyle')?.fontSize;
      if (!fs) return 16;
      const m = String(fs).match(/(\d+(?:\.\d+)?)px/);
      if (!m) return 16;
      return parseFloat(m[1]);
    } catch {
      return 16;
    }
  };

  const getCurrentLineHeight = () => {
    try {
      const p = editor?.getAttributes('paragraph')?.lineHeight;
      const h = editor?.getAttributes('heading')?.lineHeight;
      const v = p || h;
      if (!v) return 1.6;
      const num = parseFloat(String(v));
      return Number.isFinite(num) ? num : 1.6;
    } catch {
      return 1.6;
    }
  };

  const initialContentRef = useRef(contentJson ?? content);
  const applyingExternalRef = useRef(false);
  const dirtyRef = useRef(false);
  const lastPropHtmlRef = useRef(typeof content === 'string' ? content : '');
  const lastPropJsonStringRef = useRef(contentJson !== null ? jsonStringifySafe(contentJson) : null);

  const editor = useEditor(
    {
      extensions,
      content: initialContentRef.current,
      editorProps: {
        attributes: {
          class: 'prose prose-sm max-w-none focus:outline-none min-h-[200px] p-4 [&_ul]:list-disc [&_ol]:list-decimal [&_ul]:pl-6 [&_ol]:pl-6 [&_blockquote]:border-l-4 [&_blockquote]:border-border [&_blockquote]:pl-4 [&_blockquote]:py-1 [&_blockquote]:italic [&_blockquote]:text-muted-foreground [&_pre]:bg-muted [&_pre]:border [&_pre]:border-border [&_pre]:rounded [&_pre]:p-3 [&_pre]:overflow-x-auto',
        },
      },
      onUpdate: ({ editor: ed }) => {
        try {
          const html = ed.getHTML();
          const json = ed.getJSON();
          onChange?.(html, json);
        } catch (e) {
          onChange?.(ed.getHTML());
        } finally {
          if (!applyingExternalRef.current) {
            dirtyRef.current = true;
          }
        }
      },
    },
    [extensions]
  );

  useEffect(() => {
    if (!editor) return;

    if (dirtyRef.current) return;
    if (editor.view?.hasFocus?.()) return;

    if (contentJson !== null) {
      const nextJsonString = jsonStringifySafe(contentJson);
      if (nextJsonString === lastPropJsonStringRef.current) return;
      try {
        applyingExternalRef.current = true;
        editor.commands.setContent(contentJson, false);
        lastPropJsonStringRef.current = nextJsonString;
      } catch {
      } finally {
        applyingExternalRef.current = false;
      }
      return;
    }

    if (typeof content !== 'string') return;
    if (content === lastPropHtmlRef.current) return;
    try {
      applyingExternalRef.current = true;
      editor.commands.setContent(content, false);
      lastPropHtmlRef.current = content;
      lastPropJsonStringRef.current = jsonStringifySafe(editor.getJSON());
    } catch {
    } finally {
      applyingExternalRef.current = false;
    }
  }, [editor, content, contentJson]);

  if (!editor) {
    return (
      <div className="p-4 text-muted-foreground">加载编辑器...</div>
    );
  }

  return (
    <div className={`w-full h-full min-h-0 flex flex-col ${className}`}>
      <div className="flex flex-wrap items-center gap-1 border border-border bg-card rounded-t-lg px-2 py-1.5 shrink-0">
        <button className={`h-7 w-7 rounded flex items-center justify-center transition-colors ${editor.isActive('bold') ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'}`} onMouseDown={preventMouseDown} onClick={() => editor.chain().focus().toggleBold().run()} type="button" title="粗体">
          <Bold size={14} />
        </button>
        <button className={`h-7 w-7 rounded flex items-center justify-center transition-colors ${editor.isActive('italic') ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'}`} onMouseDown={preventMouseDown} onClick={() => editor.chain().focus().toggleItalic().run()} type="button" title="斜体">
          <Italic size={14} />
        </button>
        <button className={`h-7 w-7 rounded flex items-center justify-center transition-colors ${editor.isActive('strike') ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'}`} onMouseDown={preventMouseDown} onClick={() => editor.chain().focus().toggleStrike().run()} type="button" title="删除线">
          <Strikethrough size={14} />
        </button>

        <div className="w-px h-5 bg-border mx-1"></div>

        <label className="h-7 w-7 rounded flex items-center justify-center hover:bg-muted cursor-pointer transition-colors" title="字体颜色" onMouseDown={preventMouseDown}>
          <Paintbrush size={14} />
          <input
            type="color"
            className="w-0 h-0 opacity-0"
            value={textColor}
            onChange={(e) => {
              const v = e.target.value;
              setTextColor(v);
              editor.chain().focus().setColor(v).run();
            }}
          />
        </label>
        <button
          className="h-7 w-7 rounded flex items-center justify-center hover:bg-muted transition-colors"
          onMouseDown={preventMouseDown}
          type="button"
          title="清除字体颜色"
          onClick={() => editor.chain().focus().unsetColor().run()}
        >
          <Eraser size={14} />
        </button>

        <label className="h-7 w-7 rounded flex items-center justify-center hover:bg-muted cursor-pointer transition-colors" title="背景颜色" onMouseDown={preventMouseDown}>
          <Highlighter size={14} />
          <input
            type="color"
            className="w-0 h-0 opacity-0"
            value={bgColor}
            onChange={(e) => {
              const v = e.target.value;
              setBgColor(v);
              editor.chain().focus().setHighlight({ color: v }).run();
            }}
          />
        </label>
        <button
          className="h-7 w-7 rounded flex items-center justify-center hover:bg-muted transition-colors"
          onMouseDown={preventMouseDown}
          type="button"
          title="清除背景颜色"
          onClick={() => editor.chain().focus().unsetHighlight().run()}
        >
          <Ban size={14} />
        </button>

        <div className="w-px h-5 bg-border mx-1"></div>

        <button
          className="h-7 w-7 rounded flex items-center justify-center hover:bg-muted transition-colors"
          onMouseDown={preventMouseDown}
          onClick={() => {
            const next = clampFontPx(getCurrentFontPx() - 2);
            editor.chain().focus().setFontSize(`${next}px`).run();
          }}
          type="button"
          title="字体缩小"
        >
          <ZoomOut size={14} />
        </button>
        <button
          className="h-7 w-7 rounded flex items-center justify-center hover:bg-muted transition-colors"
          onMouseDown={preventMouseDown}
          onClick={() => {
            const next = clampFontPx(getCurrentFontPx() + 2);
            editor.chain().focus().setFontSize(`${next}px`).run();
          }}
          type="button"
          title="字体放大"
        >
          <ZoomIn size={14} />
        </button>
        <button
          className="h-7 w-7 rounded flex items-center justify-center hover:bg-muted transition-colors"
          onMouseDown={preventMouseDown}
          onClick={() => editor.chain().focus().unsetFontSize().run()}
          type="button"
          title="字体大小重置"
        >
          <RotateCcw size={14} />
        </button>

        <div className="w-px h-5 bg-border mx-1"></div>

        <button
          className="h-7 w-7 rounded flex items-center justify-center hover:bg-muted transition-colors"
          onMouseDown={runOnMouseDown(() => {
            const next = clampLineHeight(getCurrentLineHeight() - 0.1);
            editor.chain().focus().setLineHeight(String(next)).run();
          })}
          type="button"
          title="行距减小"
        >
          <ArrowDownToLine size={14} />
        </button>
        <button
          className="h-7 w-7 rounded flex items-center justify-center hover:bg-muted transition-colors"
          onMouseDown={runOnMouseDown(() => {
            const next = clampLineHeight(getCurrentLineHeight() + 0.1);
            editor.chain().focus().setLineHeight(String(next)).run();
          })}
          type="button"
          title="行距增大"
        >
          <ArrowUpFromLine size={14} />
        </button>
        <button
          className="h-7 w-7 rounded flex items-center justify-center hover:bg-muted transition-colors"
          onMouseDown={runOnMouseDown(() => editor.chain().focus().unsetLineHeight().run())}
          type="button"
          title="行距重置"
        >
          <RotateCcw size={14} />
        </button>

        <div className="w-px h-5 bg-border mx-1"></div>

        <button className={`h-7 w-7 rounded flex items-center justify-center transition-colors ${editor.isActive('bulletList') ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'}`} onMouseDown={runOnMouseDown(() => editor.chain().focus().toggleBulletList().run())} type="button" title="无序列表">
          <List size={14} />
        </button>
        <button className={`h-7 w-7 rounded flex items-center justify-center transition-colors ${editor.isActive('orderedList') ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'}`} onMouseDown={runOnMouseDown(() => editor.chain().focus().toggleOrderedList().run())} type="button" title="有序列表">
          <ListOrdered size={14} />
        </button>
        <button className={`h-7 w-7 rounded flex items-center justify-center transition-colors ${editor.isActive('blockquote') ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'}`} onMouseDown={runOnMouseDown(() => editor.chain().focus().toggleBlockquote().run())} type="button" title="引用">
          <Quote size={14} />
        </button>

        <div className="w-px h-5 bg-border mx-1"></div>

        <button className={`h-7 w-7 rounded flex items-center justify-center transition-colors ${editor.isActive({ textAlign: 'left' }) ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'}`} onMouseDown={preventMouseDown} onClick={() => editor.chain().focus().setTextAlign('left').run()} type="button" title="左对齐">
          <AlignLeft size={14} />
        </button>
        <button className={`h-7 w-7 rounded flex items-center justify-center transition-colors ${editor.isActive({ textAlign: 'center' }) ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'}`} onMouseDown={preventMouseDown} onClick={() => editor.chain().focus().setTextAlign('center').run()} type="button" title="居中">
          <AlignCenter size={14} />
        </button>
        <button className={`h-7 w-7 rounded flex items-center justify-center transition-colors ${editor.isActive({ textAlign: 'right' }) ? 'bg-primary text-primary-foreground' : 'hover:bg-muted'}`} onMouseDown={preventMouseDown} onClick={() => editor.chain().focus().setTextAlign('right').run()} type="button" title="右对齐">
          <AlignRight size={14} />
        </button>
      </div>

      <div className="flex-1 min-h-0 border border-t-0 border-border rounded-b-lg bg-background w-full overflow-auto">
        <EditorContent editor={editor} />
      </div>
    </div>
  );
};

export default RichTextEditor;
