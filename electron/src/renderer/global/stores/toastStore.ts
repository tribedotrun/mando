import { create } from 'zustand';

type ToastVariant = 'success' | 'error' | 'info';

export interface Toast {
  id: string;
  variant: ToastVariant;
  message: string;
}

interface ToastStore {
  toasts: Toast[];
  add: (variant: ToastVariant, message: string) => void;
  dismiss: (id: string) => void;
}

let nextId = 0;

export const useToastStore = create<ToastStore>((set) => ({
  toasts: [],

  add: (variant, message) => {
    const id = String(++nextId);
    const toast: Toast = { id, variant, message };
    set((state) => {
      const updated = [...state.toasts, toast];
      return { toasts: updated.slice(-3) };
    });
  },

  dismiss: (id) => {
    set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) }));
  },
}));
