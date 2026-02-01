import React from "react";
import Panel from "../components/Panel";

export default function AboutSection() {
  return (
    <Panel
      title="About this console"
      subtitle="Single-page admin built for the gproxy runtime."
    >
      <div className="space-y-3 text-sm text-slate-600">
        <p>
          This admin console is designed for rapid operational work: manage providers, rotate
          credentials, apply disallow rules, and inspect upstream usage in one place. Every action
          maps directly to the gproxy admin API.
        </p>
        <p>
          Use the sidebar to jump between sections. Updates are persisted immediately to the
          server, and the console will stay in sync with refresh actions.
        </p>
        <div className="rounded-xl border border-slate-200 bg-slate-50 p-3 text-xs text-slate-500">
          API endpoints: <span className="font-semibold">/admin/*</span> for management, <span className="font-semibold">/admin/upstream_usage</span> for usage analysis.
        </div>
      </div>
    </Panel>
  );
}
