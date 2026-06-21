import { Outlet, createFileRoute, redirect } from "@tanstack/react-router";
import { portalSessionQuery } from "@/api/portal";
import { PortalShell } from "@/components/shell/portal-shell";

export const Route = createFileRoute("/_portal")({
  beforeLoad: async ({ context }) => {
    try {
      const user = await context.queryClient.ensureQueryData(portalSessionQuery);
      return { user };
    } catch {
      throw redirect({ to: "/login" });
    }
  },
  component: PortalLayout,
});

function PortalLayout() {
  return (
    <PortalShell>
      <Outlet />
    </PortalShell>
  );
}
