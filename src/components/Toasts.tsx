import { AlertCircle, CheckCircle2, Info, X } from "lucide-react";

import type { ToastMessage } from "../types/ui";

interface ToastsProps {
  toasts: ToastMessage[];
  onDismiss: (id: string) => void;
}

const ICONS = {
  success: CheckCircle2,
  error: AlertCircle,
  info: Info,
};

export function Toasts({ toasts, onDismiss }: ToastsProps) {
  if (toasts.length === 0) return null;

  return (
    <div className="toast-viewport" aria-live="polite" aria-atomic="false">
      {toasts.map((toast) => {
        const Icon = ICONS[toast.tone];
        return (
          <article className={`toast toast-${toast.tone}`} key={toast.id}>
            <span className="toast-icon">
              <Icon size={17} />
            </span>
            <div className="toast-content">
              <strong>{toast.title}</strong>
              {toast.detail ? <p>{toast.detail}</p> : null}
              {toast.action && toast.actionLabel ? (
                <button
                  type="button"
                  className="toast-action"
                  onClick={() => {
                    toast.action?.();
                    onDismiss(toast.id);
                  }}
                >
                  {toast.actionLabel}
                </button>
              ) : null}
            </div>
            <button
              type="button"
              className="toast-close"
              onClick={() => onDismiss(toast.id)}
              aria-label="Dismiss notification"
            >
              <X size={14} />
            </button>
          </article>
        );
      })}
    </div>
  );
}
