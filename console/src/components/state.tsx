//! The three states every remote read has, rendered once.
//!
//! A query is loading, or it failed, or it has nothing to show. v3 spelled all
//! three out at every call site and they drifted — some pages showed a spinner
//! for a 403, some showed nothing at all. One component, three states, and the
//! error case knows what an [`ApiError`] is: a 403 is "you may not", not "it
//! broke", and saying so is the difference between a bug report and a shrug.

import { useTranslation } from "react-i18next"
import type { ReactNode } from "react"
import { ApiError } from "@/api/client"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Skeleton } from "@/components/ui/skeleton"

export function LoadingRows({ rows = 4 }: { rows?: number }) {
  return (
    <div className="space-y-2" role="status" aria-live="polite">
      {Array.from({ length: rows }, (_, index) => <Skeleton key={index} className="h-9 w-full" />)}
    </div>
  )
}

export function ErrorNotice({ error }: { error: unknown }) {
  const { t } = useTranslation()
  const api = error instanceof ApiError ? error : null
  const title = api?.status === 403 ? t("state.forbidden") : t("state.failed")
  const message = error instanceof Error ? error.message : String(error)
  // `AppError`'s own Display already begins with its code — "conflict: a team
  // named `Platform` already exists" — so repeating it would read as a stutter.
  // It is still worth showing when the message does not carry it, because the
  // code is the half a person can search for.
  const code = api && !message.startsWith(api.code) ? api.code : null
  return (
    <Alert variant="destructive">
      <AlertTitle>{title}</AlertTitle>
      <AlertDescription>
        {message}
        {code ? <span className="text-muted-foreground"> ({code})</span> : null}
      </AlertDescription>
    </Alert>
  )
}

export function EmptyNotice({ title, description }: { title: string; description?: string }) {
  return (
    <Empty>
      <EmptyHeader>
        <EmptyTitle>{title}</EmptyTitle>
        {description ? <EmptyDescription>{description}</EmptyDescription> : null}
      </EmptyHeader>
    </Empty>
  )
}

/** Loading, failed, or the children. Emptiness is the caller's business. */
export function QueryState({ isPending, error, rows, children }: {
  isPending: boolean
  error: unknown
  rows?: number
  children: ReactNode
}) {
  if (isPending) return <LoadingRows rows={rows} />
  if (error) return <ErrorNotice error={error} />
  return <>{children}</>
}
