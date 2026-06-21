import { StrictMode, Suspense } from "react";
import ReactDOM from "react-dom/client";
import { MutationCache, QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createRouter } from "@tanstack/react-router";
import { routeTree } from "./routeTree.gen";
import { ThemeProvider } from "@/lib/theme";
import { ApiError } from "@/api/http";
import "./i18n";
import "./styles/globals.css";

function handle401(error: unknown) {
  // Session expired mid-use: drop the cached session and bounce to login.
  if (error instanceof ApiError && error.status === 401) {
    queryClient.removeQueries({ queryKey: ["session"] });
    if (!router.state.location.pathname.endsWith("/login")) {
      void router.navigate({ to: "/login" });
    }
  }
}

const queryClient = new QueryClient({
  queryCache: new QueryCache({ onError: handle401 }),
  mutationCache: new MutationCache({ onError: handle401 }),
  defaultOptions: { queries: { retry: 1, refetchOnWindowFocus: false } },
});

const router = createRouter({
  routeTree,
  basepath: "/console",
  context: { queryClient },
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        <Suspense fallback={null}>
          <RouterProvider router={router} />
        </Suspense>
      </ThemeProvider>
    </QueryClientProvider>
  </StrictMode>,
);
