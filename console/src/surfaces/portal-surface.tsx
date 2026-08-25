import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { ThemeProvider } from "next-themes"
import { PortalPage } from "@/pages/portal"
import { Toaster } from "@/components/ui/sonner"
import { TooltipProvider } from "@/components/ui/tooltip"

const queryClient = new QueryClient({ defaultOptions: { queries: { staleTime: 15_000, retry: 1 } } })

export function PortalSurface() {
  return (
    <ThemeProvider attribute="class" forcedTheme="light">
      <QueryClientProvider client={queryClient}>
        <TooltipProvider><PortalPage /><Toaster /></TooltipProvider>
      </QueryClientProvider>
    </ThemeProvider>
  )
}
