import type { MouseEventHandler, ReactNode } from "react";

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
