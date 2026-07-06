import { createFileRoute } from "@tanstack/react-router";
import { RoutingRulesTab } from "@/components/providers/routing-rules-tab";

export const Route = createFileRoute("/_app/providers/$providerId/routing-rules")({
  component: RoutingRulesSection,
});

function RoutingRulesSection() {
  const { providerId } = Route.useParams();
  return <RoutingRulesTab providerId={Number(providerId)} />;
}
