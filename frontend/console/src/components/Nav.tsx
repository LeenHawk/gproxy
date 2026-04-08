import { useEffect, useMemo, useState } from "react";

export interface NavItem {
  id: string;
  label: string;
  group?: string;
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
  const groupedItems = useMemo(() => {
    const order: string[] = [];
    const groups = new Map<string, NavItem[]>();
    for (const item of items) {
      const key = item.group ?? "";
      if (!groups.has(key)) {
        groups.set(key, []);
        order.push(key);
      }
      groups.get(key)?.push(item);
    }
    return order.map((group) => ({
      group,
      items: groups.get(group) ?? [],
    }));
  }, [items]);

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
      <nav className={`sidebar-nav ${mobileOpen ? "sidebar-nav-open" : ""}`}>
        {groupedItems.map(({ group, items: groupItems }) => (
          <section key={group || "default"} className="nav-group">
            {group ? <p className="nav-group-label">{group}</p> : null}
            <div className="nav-group-items">
              {groupItems.map((item) => (
                <button
                  key={item.id}
                  className={`nav-item ${active === item.id ? "nav-item-active" : ""}`}
                  onClick={() => onChange(item.id)}
                  type="button"
                >
                  {item.label}
                </button>
              ))}
            </div>
          </section>
        ))}
      </nav>
    </aside>
  );
}
