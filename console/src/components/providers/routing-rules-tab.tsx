import { RoutingMatrix } from "./routing-matrix";

export function RoutingRulesTab({ providerId }: { providerId: number }) {
  return <RoutingMatrix providerId={providerId} />;
}
