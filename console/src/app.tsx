//! The application root: one query client, one session gate, one shell.
//!
//! The gate is the whole of the sign-in routing. There is no `/login` route
//! because there is nothing to link to: a caller with no session sees the form
//! wherever they were going, and lands on that page once the cookie exists.
//! That also means a session ending mid-visit does not lose the reader's
//! place.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { ApiError } from "@/api/client"
import { ConsoleContextProvider, useSessionContext, useUnauthorizedReset } from "@/capability/session"
import { AppShell } from "@/components/shell/app-shell"
import { ErrorNotice, LoadingRows } from "@/components/state"
import { Toaster } from "@/components/ui/sonner"
import { Routes } from "@/pages/routes"
import { SignInPage } from "@/pages/sign-in"

const client = new QueryClient({
  defaultOptions: {
    queries: {
      // A 401 has already been turned into a sign-out by `api`, and a 403 is
      // a decision rather than a fault: retrying either only delays the
      // answer. Everything else gets one more go.
      retry: (attempt, error) =>
        !(error instanceof ApiError && (error.status === 401 || error.status === 403)) && attempt < 1,
      staleTime: 10_000,
      refetchOnWindowFocus: false,
    },
  },
})

function Gate() {
  const { t } = useTranslation()
  const session = useSessionContext()
  useUnauthorizedReset()

  if (session.isPending) {
    return <main className="mx-auto max-w-2xl px-5 py-16"><LoadingRows /></main>
  }
  // A 401 is "nobody is signed in", which is a page and not an error.
  if (session.error?.status === 401 || !session.data) {
    if (session.error && session.error.status !== 401) {
      return (
        <main className="mx-auto max-w-2xl space-y-4 px-5 py-16">
          <h1 className="text-lg font-medium">{t("state.failed")}</h1>
          <ErrorNotice error={session.error} />
        </main>
      )
    }
    return <SignInPage />
  }
  return (
    <ConsoleContextProvider key={`${session.data.user.id}:${session.data.admin?.scope?.selector ?? "none"}`} context={session.data}>
      <AppShell><Routes /></AppShell>
    </ConsoleContextProvider>
  )
}

export function App() {
  return (
    <QueryClientProvider client={client}>
      <Gate />
      <Toaster />
    </QueryClientProvider>
  )
}
