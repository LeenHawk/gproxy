import type { ReactNode } from "react"
import { PageHeader } from "@/components/page-header"

export function PageLayout({ title, description, actions, children }: { title: string; description: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-7">
      <PageHeader title={title} description={description} actions={actions} />
      {children}
    </div>
  )
}
