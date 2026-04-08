import { Button, Card, Label, TextArea } from "../../../components/ui";

export function OAuthTab({
  startQuery,
  callbackQuery,
  startResult,
  callbackResult,
  onChangeStartQuery,
  onChangeCallbackQuery,
  onStart,
  onFinish,
  labels,
}: {
  startQuery: string;
  callbackQuery: string;
  startResult: string;
  callbackResult: string;
  onChangeStartQuery: (value: string) => void;
  onChangeCallbackQuery: (value: string) => void;
  onStart: () => void;
  onFinish: () => void;
  labels: {
    start: string;
    finish: string;
    startQuery: string;
    callbackQuery: string;
  };
}) {
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Card title={labels.start}>
        <Label>{labels.startQuery}</Label>
        <TextArea value={startQuery} onChange={onChangeStartQuery} rows={6} />
        <div className="mt-4">
          <Button onClick={onStart}>{labels.start}</Button>
        </div>
        {startResult ? <pre className="mt-4 overflow-auto text-xs text-muted">{startResult}</pre> : null}
      </Card>
      <Card title={labels.finish}>
        <Label>{labels.callbackQuery}</Label>
        <TextArea value={callbackQuery} onChange={onChangeCallbackQuery} rows={6} />
        <div className="mt-4">
          <Button onClick={onFinish}>{labels.finish}</Button>
        </div>
        {callbackResult ? <pre className="mt-4 overflow-auto text-xs text-muted">{callbackResult}</pre> : null}
      </Card>
    </div>
  );
}
