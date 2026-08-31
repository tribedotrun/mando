import * as React from 'react';
import { AlertDialog as AlertDialogPrimitive } from 'radix-ui';

export {
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogFooter,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogCancel,
} from '#renderer/global/ui/primitives/alert-dialog-parts';

function AlertDialog({ ...props }: React.ComponentProps<typeof AlertDialogPrimitive.Root>) {
  return <AlertDialogPrimitive.Root data-slot="alert-dialog" {...props} />;
}

export { AlertDialog };
