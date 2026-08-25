import type { ReactNode } from "react"
import { Alert, AlertTitle } from "@/components/ui/alert"
import { Empty, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Skeleton } from "@/components/ui/skeleton"

export function QueryState({ loading, error, empty, children }: { loading: boolean; error: string; empty?: string; children: ReactNode }) {
  if (loading) {
    return (
      <div className="flex flex-col gap-3" aria-busy="true">
        <Skeleton className="h-9 w-full" />
        <Skeleton className="h-28 w-full" />
        <Skeleton className="h-28 w-full" />
      </div>
    )
  }
  if (error) {
    return (
      <Alert variant="destructive">
        <AlertTitle>{error}</AlertTitle>
      </Alert>
    )
  }
  if (empty) {
    return (
      <Empty>
        <EmptyHeader>
          <EmptyTitle>{empty}</EmptyTitle>
        </EmptyHeader>
      </Empty>
    )
  }
  return children
}
