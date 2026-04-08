import { Button, Card, Input, Label } from "../../../components/ui";
import type { OAuthCallbackResponse, OAuthStartResponse } from "../../../lib/types/admin";

export function OAuthTab({
  flow,
  callbackUrl,
  callbackResult,
  onChangeCallbackUrl,
  onStart,
  onOpenAuthorize,
  onFinish,
  labels,
}: {
  flow: OAuthStartResponse | null;
  callbackUrl: string;
  callbackResult: OAuthCallbackResponse | null;
  onChangeCallbackUrl: (value: string) => void;
  onStart: () => void;
  onOpenAuthorize: () => void;
  onFinish: () => void;
  labels: {
    start: string;
    startHint: string;
    openAuthorize: string;
    redirectUri: string;
    instructions: string;
    callbackUrl: string;
    callbackHint: string;
    finish: string;
    persistedCredential: string;
  };
}) {
  return (
    <div className="grid gap-4 lg:grid-cols-[0.95fr_1.05fr]">
      <Card title={labels.start}>
        <p className="text-sm text-muted">{labels.startHint}</p>
        <div className="mt-4 flex gap-2">
          <Button onClick={onStart}>{labels.start}</Button>
          {flow?.authorize_url ? (
            <Button variant="neutral" onClick={onOpenAuthorize}>
              {labels.openAuthorize}
            </Button>
          ) : null}
        </div>
        {flow ? (
          <div className="record-list mt-4">
            <div className="record-item">
              <div className="metric-label">{labels.redirectUri}</div>
              <div className="mt-2 break-all font-mono text-xs text-text">
                {flow.redirect_uri ?? "—"}
              </div>
            </div>
            {flow.instructions ? (
              <div className="record-item">
                <div className="metric-label">{labels.instructions}</div>
                <div className="mt-2 text-sm text-muted">{flow.instructions}</div>
              </div>
            ) : null}
          </div>
        ) : null}
      </Card>

      <Card title={labels.finish}>
        <Label>{labels.callbackUrl}</Label>
        <Input value={callbackUrl} onChange={onChangeCallbackUrl} />
        <p className="mt-2 text-xs text-muted">{labels.callbackHint}</p>
        <div className="mt-4 flex gap-2">
          <Button onClick={onFinish}>{labels.finish}</Button>
        </div>
        {callbackResult ? (
          <div className="record-list mt-4">
            <div className="record-item">
              <div className="metric-label">{labels.persistedCredential}</div>
              <pre className="mt-2 overflow-auto text-xs text-muted">
                {JSON.stringify(callbackResult.credential, null, 2)}
              </pre>
            </div>
            <div className="record-item">
              <div className="metric-label">Details</div>
              <pre className="mt-2 overflow-auto text-xs text-muted">
                {JSON.stringify(callbackResult.details, null, 2)}
              </pre>
            </div>
          </div>
        ) : null}
      </Card>
    </div>
  );
}
