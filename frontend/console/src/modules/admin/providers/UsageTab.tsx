import { Button, Card } from "../../../components/ui";

export function UsageTab({
  result,
  onRefresh,
  label,
}: {
  result: string;
  onRefresh: () => void;
  label: string;
}) {
  return (
    <Card
      title="Provider Usage"
      action={
        <Button variant="neutral" onClick={onRefresh}>
          {label}
        </Button>
      }
    >
      <pre className="overflow-auto text-xs text-muted">{result || "{}"}</pre>
    </Card>
  );
}
