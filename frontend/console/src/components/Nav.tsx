import { useEffect, useMemo, useState } from "react";

export interface NavItem {
  id: string;
  label: string;
}

export function Nav({
  items,
  active,
  onChange,
}: {
  items: NavItem[];
  active: string;
  onChange: (id: string) => void;
}) {
  const [mobileOpen, setMobileOpen] = useState(false);
  const activeLabel = useMemo(
    () => items.find((item) => item.id === active)?.label ?? active,
    [active, items],
  );

  useEffect(() => {
    setMobileOpen(false);
  }, [active]);

  return (
    <aside className="sidebar-shell">
      <button
        type="button"
        className="sidebar-mobile-toggle"
        onClick={() => setMobileOpen((prev) => !prev)}
        aria-expanded={mobileOpen}
      >
        <span className="sidebar-mobile-toggle-icon" aria-hidden="true">
          <span className="sidebar-mobile-toggle-line" />
          <span className="sidebar-mobile-toggle-line" />
          <span className="sidebar-mobile-toggle-line" />
        </span>
        <span className="sidebar-mobile-toggle-label">{activeLabel}</span>
      </button>
      <nav className={`sidebar-nav space-y-1 ${mobileOpen ? "sidebar-nav-open" : ""}`}>
        {items.map((item) => (
          <button
            key={item.id}
            className={`nav-item ${active === item.id ? "nav-item-active" : ""}`}
            onClick={() => onChange(item.id)}
            type="button"
          >
            {item.label}
          </button>
        ))}
      </nav>
    </aside>
  );
}
