import React, { useRef, useState } from 'react';
import { Paperclip, X } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '#renderer/components/ui/dialog';
import { Button } from '#renderer/components/ui/button';
import { Textarea } from '#renderer/components/ui/textarea';
import { Label } from '#renderer/components/ui/label';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';

interface FeedbackModalProps {
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
  allowImages?: boolean;
  onSubmit: (feedback: string, images?: File[]) => void;
  onCancel: () => void;
}

export function FeedbackModal({
  testId,
  title,
  subtitle,
  label,
  placeholder,
  initialValue,
  buttonLabel,
  pendingLabel,
  isPending,
  requireFeedback = true,
  allowImages = false,
  onSubmit,
  onCancel,
}: FeedbackModalProps): React.ReactElement {
  const [feedback, setFeedback] = useState(initialValue ?? '');
  const [image, setImage] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  // Revoke preview URL on unmount to prevent memory leaks.
  const previewRef = useRef(preview);
  previewRef.current = preview;
  useMountEffect(() => {
    return () => {
      if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    };
  });

  const setImageFile = (file: File) => {
    if (preview) URL.revokeObjectURL(preview);
    setImage(file);
    setPreview(URL.createObjectURL(file));
  };

  const removeImage = () => {
    if (preview) URL.revokeObjectURL(preview);
    setImage(null);
    setPreview(null);
  };

  const handlePaste = (e: React.ClipboardEvent) => {
    if (!allowImages) return;
    for (const clipItem of e.clipboardData.items) {
      if (!clipItem.type.startsWith('image/')) continue;
      e.preventDefault();
      const file = clipItem.getAsFile();
      if (file) setImageFile(file);
      return;
    }
  };

  return (
    <Dialog open={true} onOpenChange={() => onCancel()}>
      <DialogContent data-testid={testId} showCloseButton={false}>
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
          onChange={(e) => setFeedback(e.target.value)}
          onPaste={handlePaste}
          autoFocus
        />

        {allowImages && preview && image && (
          <div className="flex items-center gap-2 rounded-lg bg-muted px-3 py-2">
            <img src={preview} alt={image.name} className="h-10 w-10 rounded-md object-cover" />
            <span className="min-w-0 flex-1 truncate text-[13px] text-muted-foreground">
              {image.name}
            </span>
            <Button variant="ghost" size="icon-xs" onClick={removeImage}>
              <X size={12} />
            </Button>
          </div>
        )}

        <DialogFooter>
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
                variant="ghost"
                size="icon-sm"
                onClick={() => fileRef.current?.click()}
                aria-label="Attach image"
                className="mr-auto text-muted-foreground"
              >
                <Paperclip size={16} />
              </Button>
            </>
          )}
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            onClick={() => onSubmit(feedback, image ? [image] : undefined)}
            disabled={(requireFeedback && !feedback.trim()) || isPending}
          >
            {isPending ? pendingLabel : buttonLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
