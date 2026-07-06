import { createFileRoute } from "@tanstack/react-router";
import { ProviderRulesTab } from "@/components/providers/provider-rules-tab";

export const Route = createFileRoute("/_app/providers/$providerId/rule-sets")({
  component: RuleSetsSection,
});

function RuleSetsSection() {
  const { providerId } = Route.useParams();
  return <ProviderRulesTab providerId={Number(providerId)} />;
}
