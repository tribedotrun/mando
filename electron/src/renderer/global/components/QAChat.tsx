import React, { useCallback, useRef, useState } from 'react';
import { Paperclip, X } from 'lucide-react';
import { Button } from '#renderer/components/ui/button';
import { ScrollArea } from '#renderer/components/ui/scroll-area';
import { Textarea } from '#renderer/components/ui/textarea';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';

const SCROLL_DELAY_MS = 50;

export interface QAEntry {
  role: 'user' | 'assistant';
  text: string;
}

interface QAChatProps {
  history: QAEntry[];
  pending: boolean;
  onAsk: (question: string, images?: File[]) => void;
  placeholder?: string;
  allowImages?: boolean;
  renderAnswer?: (text: string) => React.ReactNode;
  header?: React.ReactNode;
  /** Content rendered between chat history and input form (e.g. suggested follow-ups) */
  footer?: React.ReactNode;
  /** Ref that receives a scrollToBottom function */
  scrollRef?: React.MutableRefObject<(() => void) | null>;
  /** data-testid on the outer wrapper */
  testId?: string;
  /** Class names on the outer wrapper */
  className?: string;
  /** Inline style on the outer wrapper */
  style?: React.CSSProperties;
  /** Style overrides for user chat bubbles */
  userBubbleStyle?: React.CSSProperties;
  /** Style overrides for assistant chat bubbles */
  assistantBubbleStyle?: React.CSSProperties;
  /** Extra class on chat bubble <div> */
  bubbleClassName?: string;
  /** Class on the chat history scroll container */
  historyClassName?: string;
  /** Inline style on the chat history scroll container */
  historyStyle?: React.CSSProperties;
  /** Class on the input form */
  formClassName?: string;
  /** Inline style on the input form */
  formStyle?: React.CSSProperties;
}

export function QAChat({
  history,
  pending,
  onAsk,
  placeholder = 'Ask a question...',
  allowImages = false,
  renderAnswer,
  header,
  footer,
  scrollRef,
  testId,
  className = 'flex flex-col',
  style,
  userBubbleStyle,
  assistantBubbleStyle,
  bubbleClassName = '',
  historyClassName = '',
  historyStyle,
  formClassName = '',
  formStyle,
}: QAChatProps): React.ReactElement {
  const [question, setQuestion] = useState('');
  const chatEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Image state
  const [image, setImage] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const previewRef = useRef(preview);
  previewRef.current = preview;

  useMountEffect(() => {
    return () => {
      if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    };
  });

  const setImageFile = useCallback((file: File) => {
    if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    const url = URL.createObjectURL(file);
    setImage(file);
    setPreview(url);
    previewRef.current = url;
  }, []);

  const removeImage = useCallback(() => {
    if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    setImage(null);
    setPreview(null);
    previewRef.current = null;
  }, []);

  const scrollToBottom = useCallback(() => {
    setTimeout(() => chatEndRef.current?.scrollIntoView({ behavior: 'smooth' }), SCROLL_DELAY_MS);
  }, []);

  // Expose scrollToBottom to parent via ref.
  if (scrollRef) scrollRef.current = scrollToBottom;

  const handleSubmit = useCallback(
    (e: React.FormEvent) => {
      e.preventDefault();
      const q = question.trim();
      if (!q || pending) return;
      const images = image ? [image] : undefined;
      onAsk(q, images);
      setQuestion('');
      removeImage();
      if (textareaRef.current) textareaRef.current.style.height = 'auto';
      scrollToBottom();
    },
    [question, pending, onAsk, image, removeImage, scrollToBottom],
  );

  const handlePaste = useCallback(
    (e: React.ClipboardEvent) => {
      if (!allowImages) return;
      for (const clipItem of e.clipboardData.items) {
        if (!clipItem.type.startsWith('image/')) continue;
        e.preventDefault();
        const file = clipItem.getAsFile();
        if (file) setImageFile(file);
        return;
      }
    },
    [allowImages, setImageFile],
  );

  const defaultUserStyle: React.CSSProperties = {
    background: 'var(--accent)',
    border: '1px solid var(--accent)',
    color: 'var(--primary-hover)',
    whiteSpace: 'pre-wrap',
  };

  const defaultAssistantStyle: React.CSSProperties = {
    background: 'var(--secondary)',
    border: '1px solid var(--input)',
    color: 'var(--foreground)',
  };

  return (
    <div data-testid={testId} className={className} style={style}>
      {header}

      <ScrollArea className={`min-h-0 flex-1 ${historyClassName}`} style={historyStyle}>
        {history.length === 0 && (
          <div className="py-8 text-center text-xs text-text-3">{placeholder}</div>
        )}
        {history.map((entry, i) => (
          <div key={i} className={`mb-3 ${entry.role === 'user' ? 'text-right' : 'text-left'}`}>
            <div
              className={`inline-block rounded px-3 py-2 text-xs leading-relaxed ${bubbleClassName}`}
              style={
                entry.role === 'user'
                  ? { ...defaultUserStyle, ...userBubbleStyle }
                  : { ...defaultAssistantStyle, ...assistantBubbleStyle }
              }
            >
              {entry.role === 'assistant' && renderAnswer ? renderAnswer(entry.text) : entry.text}
            </div>
          </div>
        ))}
        {pending && (
          <div className="flex items-center gap-2 py-2">
            <span className="text-xs text-text-3">Thinking...</span>
          </div>
        )}
        <div ref={chatEndRef} />
      </ScrollArea>

      {footer}

      {allowImages && preview && image && (
        <div className={`flex items-center gap-1.5 ${formClassName}`}>
          <button
            type="button"
            onClick={removeImage}
            className="flex items-center gap-1.5 rounded-md bg-secondary/60 px-2 py-0.5 text-caption text-muted-foreground transition-colors hover:bg-secondary"
          >
            <img src={preview} alt="" className="h-4 w-4 rounded-sm object-cover" />
            <span className="max-w-[160px] truncate">{image.name}</span>
            <X size={10} className="shrink-0 opacity-60" />
          </button>
        </div>
      )}

      <form
        onSubmit={handleSubmit}
        className={`flex items-end gap-2 ${formClassName}`}
        style={formStyle}
      >
        {allowImages && (
          <>
            <input
              ref={fileRef}
              type="file"
              accept="image/*"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) setImageFile(file);
                e.target.value = '';
              }}
            />
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              onClick={() => fileRef.current?.click()}
              disabled={pending}
              aria-label="Attach image"
              className="shrink-0 text-muted-foreground"
            >
              <Paperclip size={14} />
            </Button>
          </>
        )}
        <Textarea
          ref={textareaRef}
          value={question}
          onChange={(e) => {
            setQuestion(e.target.value);
            e.target.style.height = 'auto';
            e.target.style.height = e.target.scrollHeight + 'px';
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              handleSubmit(e);
            }
          }}
          onPaste={handlePaste}
          placeholder={placeholder}
          className="min-h-0 flex-1 resize-none overflow-y-auto border-0 bg-muted shadow-none [scrollbar-width:none] focus-visible:ring-0 dark:bg-muted"
          rows={1}
          autoFocus
        />
        <Button type="submit" size="sm" disabled={pending || !question.trim()}>
          Ask
        </Button>
      </form>
    </div>
  );
}
