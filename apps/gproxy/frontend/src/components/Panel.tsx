import React from "react";

type PanelProps = {
  title: string;
  subtitle?: string;
  action?: React.ReactNode;
  children: React.ReactNode;
};

export default function Panel({ title, subtitle, action, children }: PanelProps) {
  return (
    <div className="panel card-shadow">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <div className="text-sm font-semibold text-slate-700">{title}</div>
          {subtitle ? <div className="mt-1 text-xs text-slate-400">{subtitle}</div> : null}
        </div>
        {action}
      </div>
      <div className="mt-4 space-y-4">{children}</div>
    </div>
  );
}
