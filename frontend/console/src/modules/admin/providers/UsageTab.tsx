import { Button, Card } from "../../../components/ui";

export function UsageTab({
  title,
  result,
  onRefresh,
  label,
}: {
  title: string;
  result: string;
  onRefresh: () => void;
  label: string;
}) {
  return (
    <Card
      title={title}
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
