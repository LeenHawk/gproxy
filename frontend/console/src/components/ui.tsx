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
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  type?: "text" | "password";
}) {
  return (
    <input
      className="input"
      value={value}
      type={type}
      placeholder={placeholder}
      onChange={(event) => onChange(event.target.value)}
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
