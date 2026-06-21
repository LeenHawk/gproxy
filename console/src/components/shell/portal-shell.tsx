import type { ReactNode } from "react";
import { AppShell } from "@/components/shell/app-shell";
import { PORTAL_NAV } from "@/components/shell/portal-nav";

export function PortalShell({ children }: { children: ReactNode }) {
  return (
    <AppShell navItems={PORTAL_NAV} contextFrom="/_portal">
      {children}
    </AppShell>
  );
}
