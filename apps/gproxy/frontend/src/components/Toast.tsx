import React from "react";

export type ToastType = "success" | "error" | "info" | "warning";

export type ToastState = {
  type: ToastType;
  message: string;
} | null;

const COLOR_MAP: Record<ToastType, string> = {
  success: "bg-emerald-600",
  error: "bg-rose-600",
  info: "bg-sky-600",
  warning: "bg-amber-500"
};

export function Toast({ toast }: { toast: ToastState }) {
  if (!toast) {
    return null;
  }

  return (
    <div
      className={`fixed right-6 top-6 z-50 rounded-2xl px-4 py-3 text-sm font-semibold text-white shadow-xl ${
        COLOR_MAP[toast.type]
      }`}
    >
      {toast.message}
    </div>
  );
}
