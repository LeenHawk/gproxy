import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEventHandler,
  type MouseEventHandler,
  type ReactNode,
} from "react";

export function Card({
  title,
  subtitle,
  action,
  children,
}: {
  title?: string;
  subtitle?: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="card-shell">
      {title || subtitle || action ? (
        <header className="mb-4 flex flex-wrap items-start justify-between gap-3">
          <div>
            {title ? <h2 className="text-lg font-semibold text-text">{title}</h2> : null}
            {subtitle ? <p className="mt-1 text-sm text-muted">{subtitle}</p> : null}
          </div>
          {action}
        </header>
      ) : null}
      {children}
    </section>
  );
}

export function Button({
  children,
  onClick,
  variant = "primary",
  type = "button",
  disabled,
}: {
  children: ReactNode;
  onClick?: MouseEventHandler<HTMLButtonElement>;
  variant?: "primary" | "neutral" | "danger";
  type?: "button" | "submit";
  disabled?: boolean;
}) {
  return (
    <button className={`btn btn-${variant}`} onClick={onClick} type={type} disabled={disabled}>
      {children}
    </button>
  );
}

export function Badge({
  children,
  variant = "neutral",
}: {
  children: ReactNode;
  variant?: "neutral" | "success" | "danger" | "accent";
}) {
  return <span className={`badge badge-${variant}`}>{children}</span>;
}

export function StatusToggle({
  label,
  checked,
  onToggle,
  checkedLabel,
  uncheckedLabel,
}: {
  label: ReactNode;
  checked: boolean;
  onToggle: () => void;
  checkedLabel: ReactNode;
  uncheckedLabel: ReactNode;
}) {
  return (
    <button
      type="button"
      className={`status-toggle ${checked ? "status-toggle-on" : "status-toggle-off"}`}
      onClick={onToggle}
      aria-pressed={checked}
    >
      <span className="status-toggle-copy">
        <span className="status-toggle-label">{label}</span>
        <span className="status-toggle-value">{checked ? checkedLabel : uncheckedLabel}</span>
      </span>
      <span className={`status-toggle-track ${checked ? "status-toggle-track-on" : ""}`}>
        <span className={`status-toggle-knob ${checked ? "status-toggle-knob-on" : ""}`} />
      </span>
    </button>
  );
}

export function Label({ children }: { children: ReactNode }) {
  return (
    <label className="mb-1 block text-xs font-semibold uppercase tracking-[0.1em] text-muted">
      {children}
    </label>
  );
}

export function Input({
  value,
  onChange,
  placeholder,
  type = "text",
  onFocus,
  onBlur,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  type?: "text" | "password";
  onFocus?: () => void;
  onBlur?: () => void;
}) {
  return (
    <input
      className="input"
      value={value}
      type={type}
      placeholder={placeholder}
      onChange={(event) => onChange(event.target.value)}
      onFocus={onFocus}
      onBlur={onBlur}
    />
  );
}

export function TextArea({
  value,
  onChange,
  rows = 5,
  placeholder,
}: {
  value: string;
  onChange: (value: string) => void;
  rows?: number;
  placeholder?: string;
}) {
  return (
    <textarea
      className="textarea"
      value={value}
      rows={rows}
      placeholder={placeholder}
      onChange={(event) => onChange(event.target.value)}
    />
  );
}

export function Select({
  value,
  onChange,
  options,
  disabled,
}: {
  value: string;
  onChange: (value: string) => void;
  options: Array<{ value: string; label: string }>;
  disabled?: boolean;
}) {
  return (
    <select
      className="select"
      value={value}
      disabled={disabled}
      onChange={(event) => onChange(event.target.value)}
    >
      {options.map((item) => (
        <option key={item.value} value={item.value}>
          {item.label}
        </option>
      ))}
    </select>
  );
}

export function SearchableSelect({
  value,
  onChange,
  options,
  placeholder,
  disabled,
  noResultLabel = "No matches",
}: {
  value: string;
  onChange: (value: string) => void;
  options: Array<{ value: string; label: string }>;
  placeholder?: string;
  disabled?: boolean;
  noResultLabel?: string;
}) {
  const blurTimer = useRef<number | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(
    () => () => {
      if (blurTimer.current !== null) {
        window.clearTimeout(blurTimer.current);
      }
    },
    [],
  );

  const filteredOptions = useMemo(() => {
    const needle = value.trim().toLowerCase();
    if (!needle) {
      return options;
    }
    return options.filter(
      (item) =>
        item.label.toLowerCase().includes(needle) || item.value.toLowerCase().includes(needle),
    );
  }, [options, value]);

  const handleBlur = () => {
    blurTimer.current = window.setTimeout(() => {
      setOpen(false);
    }, 120);
  };

  const handleKeyDown: KeyboardEventHandler<HTMLInputElement> = (event) => {
    if (event.key === "Escape") {
      setOpen(false);
      return;
    }
    if (event.key === "Enter") {
      const first = filteredOptions[0];
      if (first) {
        event.preventDefault();
        onChange(first.value);
        setOpen(false);
      }
    }
  };

  return (
    <div className="search-select">
      <input
        className="input"
        value={value}
        disabled={disabled}
        placeholder={placeholder}
        onChange={(event) => {
          onChange(event.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onBlur={handleBlur}
        onKeyDown={handleKeyDown}
      />
      {open && !disabled ? (
        <div className="search-select-list">
          {filteredOptions.length > 0 ? (
            filteredOptions.map((item) => (
              <button
                key={item.value}
                type="button"
                className="search-select-item"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => {
                  onChange(item.value);
                  setOpen(false);
                }}
              >
                {item.label}
              </button>
            ))
          ) : (
            <div className="search-select-empty">{noResultLabel}</div>
          )}
        </div>
      ) : null}
    </div>
  );
}

/// Lightweight grid `Table` that takes a list of column headers and a list of
/// row dictionaries keyed by header. Skipped cells render as `—`. Used by
/// the requests / usages / my-usage modules to display tabular data with
/// uniform spacing.
export function Table({
  columns,
  rows,
}: {
  columns: string[];
  rows: Array<Record<string, ReactNode>>;
}) {
  return (
    <div className="overflow-x-auto">
      <table className="ui-table w-full text-left text-sm">
        <thead>
          <tr>
            {columns.map((column) => (
              <th
                key={column}
                className="border-b border-border px-3 py-2 text-xs font-semibold uppercase tracking-[0.08em] text-muted"
              >
                {column}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.length === 0 ? (
            <tr>
              <td className="px-3 py-4 text-center text-sm text-muted" colSpan={columns.length}>
                —
              </td>
            </tr>
          ) : (
            rows.map((row, index) => (
              <tr key={index} className="border-b border-border/40 align-top">
                {columns.map((column) => (
                  <td key={column} className="px-3 py-2 text-text">
                    {row[column] ?? "—"}
                  </td>
                ))}
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  );
}

/// Compact metric card used in dashboards to display a labeled count or
/// token total. Pair with a grid container for the layout.
export function MetricCard({
  label,
  value,
}: {
  label: ReactNode;
  value: ReactNode;
}) {
  return (
    <div className="metric-card">
      <div className="metric-label">{label}</div>
      <div className="metric-value">{value}</div>
    </div>
  );
}
