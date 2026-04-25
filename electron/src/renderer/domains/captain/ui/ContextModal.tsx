import React from 'react';
import { X } from 'lucide-react';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';
import {
  Dialog,
  DialogContentPlain,
  DialogHeader,
  DialogTitle,
  DialogClose,
} from '#renderer/global/ui/primitives/dialog';
import { Button } from '#renderer/global/ui/primitives/button';

export function ContextModal({
  context,
  onClose,
}: {
  context: string;
  onClose: () => void;
}): React.ReactElement {
  return (
    <Dialog
      open={true}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContentPlain
        data-testid="context-modal"
        className="flex max-h-[70vh] w-[560px] max-w-[90vw] flex-col p-0"
      >
        <div className="flex shrink-0 items-center justify-between px-5 pt-4 pb-3">
          <DialogHeader className="flex-1">
            <DialogTitle className="mb-0">Context</DialogTitle>
          </DialogHeader>
          <DialogClose asChild>
            <Button variant="ghost" size="icon-xs" aria-label="Close context">
              <X size={14} />
            </Button>
          </DialogClose>
        </div>
        <div className="min-w-0 overflow-y-auto px-5 pb-5 [overflow-wrap:anywhere]">
          <PrMarkdown text={context} />
        </div>
      </DialogContentPlain>
    </Dialog>
  );
}
