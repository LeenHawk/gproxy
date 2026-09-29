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
import { Empty, EmptyHeader, EmptyTitle } from "@/components/ui/empty"
import { Skeleton } from "@/components/ui/skeleton"

export function LoadingRows({ rows = 4 }: { rows?: number }) {
  const { t } = useTranslation()
  return (
    <div className="space-y-2" role="status" aria-live="polite">
      <span className="sr-only">{t("state.loading")}</span>
      {Array.from({ length: rows }, (_, index) => <Skeleton aria-hidden="true" key={index} className="h-9 w-full" />)}
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
      {/*
        This text comes from the server and routinely carries an id —
        "refused by rule 9d36240f…" — with nowhere to break. Unbroken it
        stretched the alert's grid to 584px inside a 390px viewport and
        scrolled the document sideways, on whichever page or dialog happened
        to be showing the failure.

        `wrap-anywhere` and not `break-words`: the latter wraps the line but
        leaves the min-content width at the whole token, and a grid track
        cannot shrink below that — the text would wrap inside a box that is
        still too wide. `anywhere` shrinks the box too, and still prefers a
        space, so only the token breaks and the prose around it reads.
      */}
      <AlertDescription className="wrap-anywhere">
        {message}
        {code ? <span className="text-muted-foreground"> ({code})</span> : null}
      </AlertDescription>
    </Alert>
  )
}

export function EmptyNotice({ title }: { title: string }) {
  return (
    <Empty>
      <EmptyHeader>
        <EmptyTitle>{title}</EmptyTitle>
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
