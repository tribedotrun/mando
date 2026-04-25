import * as React from 'react';
import { XIcon } from 'lucide-react';
import { Dialog as DialogPrimitive } from 'radix-ui';

import { cn } from '#renderer/global/service/cn';
import { Button } from '#renderer/global/ui/primitives/button';
import { DialogOverlay, DialogPortal } from '#renderer/global/ui/primitives/dialog';

function DialogChromeCloseButton(): React.ReactElement {
  return (
    <DialogPrimitive.Close
      data-slot="dialog-close"
      className="absolute top-4 right-4 rounded-xs opacity-70 ring-offset-background transition-opacity hover:opacity-100 focus:ring-2 focus:ring-ring focus:ring-offset-2 focus:outline-hidden disabled:pointer-events-none data-[state=open]:bg-accent data-[state=open]:text-muted-foreground [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4"
    >
      <XIcon />
      <span className="sr-only">Close</span>
    </DialogPrimitive.Close>
  );
}

function DialogContentFrame({
  className,
  children,
  closeButton,
  ...props
}: React.ComponentProps<typeof DialogPrimitive.Content> & {
  closeButton?: React.ReactNode;
}) {
  return (
    <DialogPortal data-slot="dialog-portal">
      <DialogOverlay />
      <DialogPrimitive.Content
        data-slot="dialog-content"
        className={cn(
          'fixed top-[50%] left-[50%] z-50 grid w-full max-w-[calc(100%-2rem)] translate-x-[-50%] translate-y-[-50%] gap-4 rounded-lg bg-background p-6 shadow-lg duration-200 outline-none data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95 sm:max-w-lg',
          className,
        )}
        {...props}
      >
        {children}
        {closeButton}
      </DialogPrimitive.Content>
    </DialogPortal>
  );
}

function DialogFooterCloseButton({ children = 'Close' }: { children?: React.ReactNode }) {
  return (
    <DialogPrimitive.Close asChild>
      <Button type="button" variant="outline">
        {children}
      </Button>
    </DialogPrimitive.Close>
  );
}

export { DialogChromeCloseButton, DialogContentFrame, DialogFooterCloseButton };
