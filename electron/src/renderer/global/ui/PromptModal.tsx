import React, { useRef } from 'react';
import { Paperclip, X } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';
import { useTextImageDraft } from '#renderer/global/runtime/useTextImageDraft';
import { extractImageFromClipboard } from '#renderer/global/service/clipboardImage';
import { runAndClearOnSuccess } from '#renderer/global/service/runAndClearOnSuccess';
import { PromptModalFrame } from '#renderer/global/ui/PromptModalFrame';

interface PromptModalBaseProps {
  testId: string;
  title: string;
  subtitle?: string;
  label?: string;
  placeholder: string;
  initialValue?: string;
  buttonLabel: string;
  pendingLabel: string;
  isPending: boolean;
  requirePrompt?: boolean;
  onCancel: () => void;
  /** Persistence key for draft text (and image, if using ImagePromptModal). */
  draftKey: string;
}

interface TextPromptModalProps extends PromptModalBaseProps {
  /**
   * Return a Promise to have the modal clear its draft only on resolve (and
   * preserve the draft for retry on reject). Sync/void callers get eager
   * clearing.
   */
  onSubmit: (prompt: string) => Promise<void> | void;
}

interface ImagePromptModalProps extends PromptModalBaseProps {
  /** See {@link TextPromptModalProps.onSubmit}. */
  onSubmit: (prompt: string, images?: File[]) => Promise<void> | void;
}

export function PromptModal({
  initialValue,
  onSubmit,
  draftKey,
  ...props
}: TextPromptModalProps): React.ReactElement {
  const {
    text: prompt,
    setText: setPrompt,
    clearDraftStorage,
  } = useTextImageDraft(draftKey, { initialText: initialValue });

  return (
    <PromptModalFrame
      {...props}
      prompt={prompt}
      setPrompt={setPrompt}
      onSubmitClick={() => {
        void runAndClearOnSuccess(onSubmit(prompt), clearDraftStorage);
      }}
    />
  );
}

export function ImagePromptModal({
  initialValue,
  onSubmit,
  draftKey,
  ...props
}: ImagePromptModalProps): React.ReactElement {
  const {
    text: prompt,
    setText: setPrompt,
    image,
    preview,
    setImageFile,
    removeImage,
    clearDraftStorage,
  } = useTextImageDraft(draftKey, { initialText: initialValue });
  const fileRef = useRef<HTMLInputElement>(null);

  return (
    <PromptModalFrame
      {...props}
      prompt={prompt}
      setPrompt={setPrompt}
      onPaste={(event) => {
        const file = extractImageFromClipboard(event);
        if (file) setImageFile(file);
      }}
      attachmentPreview={
        preview && image ? (
          <div className="flex items-center gap-2 rounded-lg bg-muted px-3 py-2">
            <img src={preview} alt={image.name} className="h-10 w-10 rounded-md object-cover" />
            <span className="min-w-0 flex-1 truncate text-[13px] text-muted-foreground">
              {image.name}
            </span>
            <Button variant="ghost" size="icon-xs" onClick={removeImage}>
              <X size={12} />
            </Button>
          </div>
        ) : undefined
      }
      attachmentButton={
        <>
          <input
            ref={fileRef}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={(event) => {
              const file = event.target.files?.[0];
              if (file) setImageFile(file);
              event.target.value = '';
            }}
          />
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => fileRef.current?.click()}
            aria-label="Attach image"
            className="mr-auto text-muted-foreground"
          >
            <Paperclip size={16} />
          </Button>
        </>
      }
      onSubmitClick={() => {
        void runAndClearOnSuccess(onSubmit(prompt, image ? [image] : undefined), clearDraftStorage);
      }}
    />
  );
}
