import { useSuspenseQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { providerQuery } from "@/api/providers";
import { ProviderForm } from "@/components/providers/provider-form";

export const Route = createFileRoute("/_app/providers/$providerId/settings")({
  component: SettingsSection,
});

function SettingsSection() {
  const { providerId } = Route.useParams();
  const { data: provider } = useSuspenseQuery(providerQuery(Number(providerId)));
  return (
    <div className="max-w-2xl">
      <ProviderForm provider={provider} onSaved={() => void 0} />
    </div>
  );
}
