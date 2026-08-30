import type { ReactNode } from "react"
import { cn } from "@/lib/utils"

// A page section is separated by space and titled by a heading. A border is for
// something an operator acts on as a unit — not for every group of fields.
// The description is optional on purpose: a heading that carries its own meaning
// does not need a caption under it.
export function Section({ title, description, actions, className, children }: {
  title: string
  description?: string
  actions?: ReactNode
  className?: string
  children: ReactNode
}) {
  return (
    <section className={cn("flex flex-col gap-4", className)}>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="text-base font-semibold">{title}</h2>
          {description ? <p className="mt-1 text-sm text-muted-foreground">{description}</p> : null}
        </div>
        {actions}
      </div>
      {children}
    </section>
  )
}
