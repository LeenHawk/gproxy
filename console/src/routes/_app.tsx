import { Outlet, createFileRoute, redirect } from "@tanstack/react-router";
import { sessionQuery } from "@/api/auth";
import { AppShell } from "@/components/shell/app-shell";

export const Route = createFileRoute("/_app")({
  beforeLoad: async ({ context }) => {
    try {
      const user = await context.queryClient.ensureQueryData(sessionQuery);
      return { user };
    } catch {
      throw redirect({ to: "/login" });
    }
  },
  component: AppLayout,
});

function AppLayout() {
  return (
    <AppShell>
      <Outlet />
    </AppShell>
  );
}
