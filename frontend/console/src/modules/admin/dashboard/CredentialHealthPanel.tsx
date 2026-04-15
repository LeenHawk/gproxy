import { Card } from "../../../components/ui";
import type { CredentialHealthRow } from "../../../lib/types/admin";

const STATUS_CONFIG: { key: string; dot: string; fallbackLabel: string }[] = [
  { key: "healthy", dot: "bg-emerald-500", fallbackLabel: "Healthy" },
  { key: "cooldown", dot: "bg-amber-500", fallbackLabel: "Cooldown" },
  { key: "dead", dot: "bg-rose-500", fallbackLabel: "Dead" },
];

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
  const counts: Record<string, number> = {};
  for (const row of rows) {
    const key = row.status === "healthy" || row.status === "cooldown" ? row.status : "dead";
    counts[key] = (counts[key] ?? 0) + 1;
  }

  return (
    <Card title={labels.title} subtitle={error ?? undefined}>
      <div className="flex flex-wrap gap-6">
        {STATUS_CONFIG.map(({ key, dot, fallbackLabel }) => (
          <div key={key} className="flex items-center gap-2 text-sm">
            <span className={`inline-block h-3 w-3 rounded-full ${dot}`} />
            <span className="text-secondary">{(labels as Record<string, string>)[key] ?? fallbackLabel}</span>
            <span className="font-semibold">{counts[key] ?? 0}</span>
          </div>
        ))}
      </div>
    </Card>
  );
}
