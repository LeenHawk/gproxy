export interface ToastState {
  kind: "success" | "error" | "info";
  message: string;
}

export function Toast({ toast }: { toast: ToastState | null }) {
  if (!toast) {
    return null;
  }
  return (
    <div className="fixed right-4 top-4 z-50">
      <div className={`toast toast-${toast.kind}`}>{toast.message}</div>
    </div>
  );
}
