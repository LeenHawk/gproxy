//! The page frame: a heading, an optional action, and the body under it.

import type { ReactNode } from "react"
import { cn } from "@/lib/utils"

export function PageHeader({ title, actions }: {
  title: string
  actions?: ReactNode
}) {
  return (
    <header className="flex flex-wrap items-start justify-between gap-3 border-b border-border pb-4">
      <h1 className="text-lg font-medium tracking-tight">{title}</h1>
      {actions ? <div className="flex flex-wrap items-center gap-2">{actions}</div> : null}
    </header>
  )
}

export function Page({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("space-y-6", className)}>{children}</div>
}

/** A labelled band inside a page, for the pages that hold more than one thing. */
export function PageSection({ title, actions, children }: {
  title: string
  actions?: ReactNode
  children: ReactNode
}) {
  return (
    <section className="space-y-3">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="text-sm font-medium">{title}</h2>
        {actions ? <div className="flex items-center gap-2">{actions}</div> : null}
      </div>
      {children}
    </section>
  )
}
