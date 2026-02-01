import React from "react";

export type NavItem = {
  id: string;
  label: string;
  description?: string;
  badge?: string;
};

type SidebarProps = {
  items: NavItem[];
  active: string;
  onChange: (id: string) => void;
};

const ICONS: Record<string, React.ReactNode> = {
  overview: (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M3 12h7V3H3v9Z" />
      <path d="M14 21h7v-6h-7v6Z" />
      <path d="M14 10h7V3h-7v7Z" />
      <path d="M3 21h7v-6H3v6Z" />
    </svg>
  ),
  channels: (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M4 6h16M4 12h16M4 18h16" />
      <circle cx="8" cy="6" r="1" />
      <circle cx="16" cy="12" r="1" />
      <circle cx="8" cy="18" r="1" />
    </svg>
  ),
  disallow: (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth="1.5">
      <circle cx="12" cy="12" r="9" />
      <path d="M8 8l8 8" />
    </svg>
  ),
  users: (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth="1.5">
      <circle cx="9" cy="8" r="3" />
      <circle cx="16" cy="10" r="2" />
      <path d="M3 19c0-3.3 2.7-6 6-6" />
      <path d="M14 20c.4-2.2 2.1-4 4-4" />
    </svg>
  ),
  keys: (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth="1.5">
      <circle cx="7" cy="15" r="4" />
      <path d="M11 15h10M17 15v-3M20 15v-3" />
    </svg>
  ),
  stats: (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M5 12v7M12 8v11M19 4v15" />
    </svg>
  ),
  usage: (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M3 12h5l2 5 4-10 2 5h5" />
    </svg>
  ),
  logs: (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M7 7h10M7 12h10M7 17h6" />
      <rect x="3" y="5" width="2" height="2" rx="0.5" />
      <rect x="3" y="10" width="2" height="2" rx="0.5" />
      <rect x="3" y="15" width="2" height="2" rx="0.5" />
    </svg>
  ),
  config: (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth="1.5">
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V22a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H2a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3h.1a1.7 1.7 0 0 0 1-1.5V2a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8v.1a1.7 1.7 0 0 0 1.5 1H22a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z" />
    </svg>
  ),
  about: (
    <svg viewBox="0 0 24 24" className="h-5 w-5" fill="none" stroke="currentColor" strokeWidth="1.5">
      <circle cx="12" cy="12" r="9" />
      <path d="M12 8h.01" />
      <path d="M11 12h1v4h1" />
    </svg>
  )
};

export default function Sidebar({ items, active, onChange }: SidebarProps) {
  return (
    <aside className="card card-shadow flex w-full flex-col gap-4 p-4 md:w-64 md:shrink-0">
      <div>
        <div className="text-sm uppercase tracking-[0.3em] text-slate-400">gproxy</div>
        <div className="text-xl font-semibold text-slate-900">Admin Console</div>
        <div className="mt-2 text-xs text-slate-500">
          Control plane for providers, credentials, and traffic policy.
        </div>
      </div>
      <nav className="flex gap-2 overflow-x-auto md:flex-col md:overflow-visible">
        {items.map((item) => {
          const isActive = item.id === active;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => onChange(item.id)}
              className={`flex items-center gap-3 rounded-2xl px-4 py-3 text-left transition-all ${
                isActive
                  ? "bg-slate-900 text-white shadow-lg"
                  : "text-slate-600 hover:bg-slate-100"
              }`}
            >
              <span className={isActive ? "text-white" : "text-slate-500"}>
                {ICONS[item.id] ?? ICONS.overview}
              </span>
              <span className="flex flex-col items-start gap-0.5">
                <span className="text-sm font-semibold">{item.label}</span>
                {item.description ? (
                  <span className={`text-[0.7rem] ${isActive ? "text-slate-200" : "text-slate-400"}`}>
                    {item.description}
                  </span>
                ) : null}
              </span>
              {item.badge ? (
                <span
                  className={`ml-auto rounded-full border px-2 py-0.5 text-[0.65rem] font-semibold ${
                    isActive
                      ? "border-white/40 text-white"
                      : "border-slate-200 text-slate-400"
                  }`}
                >
                  {item.badge}
                </span>
              ) : null}
            </button>
          );
        })}
      </nav>
    </aside>
  );
}
