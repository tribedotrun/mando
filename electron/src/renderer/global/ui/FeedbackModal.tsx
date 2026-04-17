import React, { useRef, useState } from 'react';
import { Paperclip, X } from 'lucide-react';
import {
  Dialog,
  DialogContentPlain,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '#renderer/global/ui/dialog';
import { Button } from '#renderer/global/ui/button';
import { Textarea } from '#renderer/global/ui/textarea';
import { Label } from '#renderer/global/ui/label';
import { useImageAttachment } from '#renderer/global/runtime/useImageAttachment';
import { extractImageFromClipboard } from '#renderer/global/service/clipboardImage';

interface FeedbackModalBaseProps {
  testId: string;
  title: string;
  subtitle?: string;
  label?: string;
  placeholder: string;
  initialValue?: string;
  buttonLabel: string;
  pendingLabel: string;
  isPending: boolean;
  requireFeedback?: boolean;
  onCancel: () => void;
}

interface TextFeedbackModalProps extends FeedbackModalBaseProps {
  onSubmit: (feedback: string) => void;
}

interface ImageFeedbackModalProps extends FeedbackModalBaseProps {
  onSubmit: (feedback: string, images?: File[]) => void;
}

interface FeedbackModalFrameProps extends FeedbackModalBaseProps {
  feedback: string;
  setFeedback: React.Dispatch<React.SetStateAction<string>>;
  onSubmitClick: () => void;
  onPaste?: (event: React.ClipboardEvent) => void;
  attachmentPreview?: React.ReactNode;
  attachmentButton?: React.ReactNode;
}

function FeedbackModalFrame({
  testId,
  title,
  subtitle,
  label,
  placeholder,
  buttonLabel,
  pendingLabel,
  isPending,
  requireFeedback = true,
  onCancel,
  feedback,
  setFeedback,
  onSubmitClick,
  onPaste,
  attachmentPreview,
  attachmentButton,
}: FeedbackModalFrameProps): React.ReactElement {
  return (
    <Dialog open={true} onOpenChange={() => onCancel()}>
      <DialogContentPlain data-testid={testId}>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          {subtitle && (
            <DialogDescription className="truncate" title={subtitle}>
              {subtitle}
            </DialogDescription>
          )}
        </DialogHeader>

        {label && <Label className="text-muted-foreground">{label}</Label>}
        <Textarea
          className="min-h-[80px]"
          rows={3}
          placeholder={placeholder}
          value={feedback}
          onChange={(event) => setFeedback(event.target.value)}
          onPaste={onPaste}
          autoFocus
        />

        {attachmentPreview}

        <DialogFooter>
          {attachmentButton}
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            onClick={onSubmitClick}
            disabled={(requireFeedback && !feedback.trim()) || isPending}
          >
            {isPending ? pendingLabel : buttonLabel}
          </Button>
        </DialogFooter>
      </DialogContentPlain>
    </Dialog>
  );
}

export function FeedbackModal({
  initialValue,
  onSubmit,
  ...props
}: TextFeedbackModalProps): React.ReactElement {
  const [feedback, setFeedback] = useState(initialValue ?? '');

  return (
    <FeedbackModalFrame
      {...props}
      feedback={feedback}
      setFeedback={setFeedback}
      onSubmitClick={() => onSubmit(feedback)}
    />
  );
}

export function ImageFeedbackModal({
  initialValue,
  onSubmit,
  ...props
}: ImageFeedbackModalProps): React.ReactElement {
  const [feedback, setFeedback] = useState(initialValue ?? '');
  const { image, preview, setImageFile, removeImage } = useImageAttachment();
  const fileRef = useRef<HTMLInputElement>(null);

  return (
    <FeedbackModalFrame
      {...props}
      feedback={feedback}
      setFeedback={setFeedback}
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
      onSubmitClick={() => onSubmit(feedback, image ? [image] : undefined)}
    />
  );
}
