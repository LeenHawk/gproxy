import { Card } from "../../../components/ui";
import type { CredentialHealthRow } from "../../../lib/types/admin";

const STATUS_KEYS = ["healthy", "cooldown", "dead"] as const;
const DOT_COLORS: Record<string, string> = {
  healthy: "bg-emerald-500",
  cooldown: "bg-amber-500",
  dead: "bg-rose-500",
};

type ProviderCounts = Record<string, number>;

export function CredentialHealthPanel({
  rows,
  error,
  labels,
}: {
  rows: CredentialHealthRow[];
  error: string | null;
  labels: {
    title: string;
    healthy: string;
    cooldown: string;
    dead: string;
  };
}) {
  // Group by provider → { healthy, cooldown, dead }
  const grouped = new Map<string, ProviderCounts>();
  for (const row of rows) {
    const key = row.status === "healthy" || row.status === "cooldown" ? row.status : "dead";
    let counts = grouped.get(row.provider);
    if (!counts) {
      counts = { healthy: 0, cooldown: 0, dead: 0 };
      grouped.set(row.provider, counts);
    }
    counts[key]++;
  }

  const providers = [...grouped.entries()].sort((a, b) => a[0].localeCompare(b[0]));

  return (
    <Card title={labels.title} subtitle={error ?? undefined}>
      {providers.length === 0 ? (
        <p className="text-sm text-secondary">—</p>
      ) : (
        <div className="divide-y divide-border">
          {providers.map(([provider, counts]) => (
            <div key={provider} className="flex items-center gap-4 py-2 first:pt-0 last:pb-0">
              <span className="min-w-30 text-sm font-medium truncate" title={provider}>
                {provider}
              </span>
              <div className="flex flex-wrap gap-4">
                {STATUS_KEYS.map((sk) => (
                  <div key={sk} className="flex items-center gap-1.5 text-sm">
                    <span className={`inline-block h-2.5 w-2.5 rounded-full ${DOT_COLORS[sk]}`} />
                    <span className="text-secondary">{labels[sk]}</span>
                    <span className="font-semibold">{counts[sk]}</span>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}
