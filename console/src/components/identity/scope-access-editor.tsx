import { type Scope } from "@/api/authz";
import { Separator } from "@/components/ui/separator";
import { PermissionsSection } from "./scope-access/permissions-section";
import { RateLimitsSection } from "./scope-access/rate-limits-section";
import { QuotaSection } from "./scope-access/quota-section";

export function ScopeAccessEditor({ scope, scopeId }: { scope: Scope; scopeId: number }) {
  return (
    <div className="grid gap-6">
      <PermissionsSection scope={scope} scopeId={scopeId} />
      <Separator />
      <RateLimitsSection scope={scope} scopeId={scopeId} />
      <Separator />
      <QuotaSection scope={scope} scopeId={scopeId} />
    </div>
  );
}
