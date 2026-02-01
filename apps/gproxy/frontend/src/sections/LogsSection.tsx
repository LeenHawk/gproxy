import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Panel from "../components/Panel";
import JsonBlock from "../components/JsonBlock";
import { apiErrorMessage, apiRequest } from "../lib/api";
import { formatOptional, formatTimestamp, maskValue } from "../lib/format";
import type { DownstreamLog, LogPage, UpstreamLog } from "../lib/types";

type Notify = (toast: { type: "success" | "error" | "info" | "warning"; message: string }) => void;

type LogKind = "downstream" | "upstream";

type LogFeedProps = {
  adminKey: string;
  notify: Notify;
  kind: LogKind;
  title: string;
  subtitle: string;
};

const PAGE_SIZE = 24;

function safeJsonParse(value: string) {
  if (!value) {
    return { kind: "empty" as const, value: "" };
  }
  try {
    return { kind: "json" as const, value: JSON.parse(value) };
  } catch {
    return { kind: "text" as const, value };
  }
}

function StatusBadge({ status }: { status: number }) {
  const tone =
    status >= 200 && status < 300
      ? "border-emerald-200 text-emerald-600"
      : status >= 400
      ? "border-rose-200 text-rose-600"
      : "border-amber-200 text-amber-600";
  return <span className={`badge ${tone}`}>{status}</span>;
}

function PayloadBlock({ label, value }: { label: string; value: string }) {
  const parsed = safeJsonParse(value);
  return (
    <div>
      <div className="label">{label}</div>
      {parsed.kind === "json" ? (
        <JsonBlock value={parsed.value} />
      ) : (
        <pre className="whitespace-pre-wrap rounded-xl border border-slate-200 bg-slate-50 p-3 text-xs text-slate-700">
          {parsed.kind === "empty" ? "-" : parsed.value}
        </pre>
      )}
    </div>
  );
}

function upstreamTokenSummary(log: UpstreamLog) {
  const tokens = [
    ["Claude", log.claude_total_tokens],
    ["Gemini", log.gemini_total_tokens],
    ["OpenAI Chat", log.openai_chat_total_tokens],
    ["OpenAI Responses", log.openai_responses_total_tokens]
  ].filter(([, value]) => value !== null && value !== undefined);

  if (tokens.length === 0) {
    return "-";
  }
  return tokens.map(([label, value]) => `${label}: ${value}`).join(" · ");
}

function DownstreamCard({ log }: { log: DownstreamLog }) {
  return (
    <div className="card border border-slate-200/80 p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <div className="text-sm font-semibold text-slate-800">
            {formatTimestamp(log.created_at)} · {log.provider}
          </div>
          <div className="mt-1 text-xs text-slate-500">
            {log.request_method} {log.request_path}
            {log.request_query ? `?${log.request_query}` : ""}
          </div>
        </div>
        <StatusBadge status={log.response_status} />
      </div>
      <div className="mt-3 grid gap-2 text-xs text-slate-500 md:grid-cols-4">
        <div>
          <div className="label">Operation</div>
          <div className="text-slate-700">{formatOptional(log.operation)}</div>
        </div>
        <div>
          <div className="label">Model</div>
          <div className="text-slate-700">{formatOptional(log.model)}</div>
        </div>
        <div>
          <div className="label">User</div>
          <div className="text-slate-700">{formatOptional(log.user_id)}</div>
        </div>
        <div>
          <div className="label">Key</div>
          <div className="text-slate-700">{formatOptional(log.key_id)}</div>
        </div>
        <div className="md:col-span-4">
          <div className="label">Trace</div>
          <div className="text-slate-700">{log.trace_id ? maskValue(log.trace_id, 8, 6) : "-"}</div>
        </div>
      </div>
      <details className="mt-3 rounded-xl border border-slate-200 bg-white/70 p-3">
        <summary className="cursor-pointer text-xs font-semibold text-slate-500">
          View payload
        </summary>
        <div className="mt-3 grid gap-3 md:grid-cols-2">
          <PayloadBlock label="Request headers" value={log.request_headers} />
          <PayloadBlock label="Response headers" value={log.response_headers} />
          <PayloadBlock label="Request body" value={log.request_body} />
          <PayloadBlock label="Response body" value={log.response_body} />
        </div>
      </details>
    </div>
  );
}

function UpstreamCard({ log }: { log: UpstreamLog }) {
  return (
    <div className="card border border-slate-200/80 p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <div className="text-sm font-semibold text-slate-800">
            {formatTimestamp(log.created_at)} · {log.provider}
          </div>
          <div className="mt-1 text-xs text-slate-500">
            {log.request_method} {log.request_path}
            {log.request_query ? `?${log.request_query}` : ""}
          </div>
        </div>
        <StatusBadge status={log.response_status} />
      </div>
      <div className="mt-3 grid gap-2 text-xs text-slate-500 md:grid-cols-4">
        <div>
          <div className="label">Operation</div>
          <div className="text-slate-700">{formatOptional(log.operation)}</div>
        </div>
        <div>
          <div className="label">Model</div>
          <div className="text-slate-700">{formatOptional(log.model)}</div>
        </div>
        <div>
          <div className="label">Credential</div>
          <div className="text-slate-700">{formatOptional(log.credential_id)}</div>
        </div>
        <div>
          <div className="label">Tokens</div>
          <div className="text-slate-700">{upstreamTokenSummary(log)}</div>
        </div>
        <div className="md:col-span-4">
          <div className="label">Trace</div>
          <div className="text-slate-700">{log.trace_id ? maskValue(log.trace_id, 8, 6) : "-"}</div>
        </div>
      </div>
      <details className="mt-3 rounded-xl border border-slate-200 bg-white/70 p-3">
        <summary className="cursor-pointer text-xs font-semibold text-slate-500">
          View payload
        </summary>
        <div className="mt-3 grid gap-3 md:grid-cols-2">
          <PayloadBlock label="Request headers" value={log.request_headers} />
          <PayloadBlock label="Response headers" value={log.response_headers} />
          <PayloadBlock label="Request body" value={log.request_body} />
          <PayloadBlock label="Response body" value={log.response_body} />
        </div>
      </details>
    </div>
  );
}

function LogFeed({ adminKey, notify, kind, title, subtitle }: LogFeedProps) {
  const [items, setItems] = useState<Array<UpstreamLog | DownstreamLog>>([]);
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(true);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const sentinelRef = useRef<HTMLDivElement | null>(null);

  const loadPage = useCallback(
    async (nextPage: number, append: boolean) => {
      if (loading) {
        return;
      }
      setLoading(true);
      setError(null);
      try {
        const data = await apiRequest<LogPage<UpstreamLog | DownstreamLog>>(
          `/admin/logs/${kind}?page=${nextPage}&page_size=${PAGE_SIZE}`,
          { adminKey }
        );
        setItems((prev) => (append ? [...prev, ...data.items] : data.items));
        setPage(nextPage);
        setHasMore(data.has_more);
      } catch (err) {
        const message = apiErrorMessage(err);
        setError(message);
        notify({ type: "error", message });
      } finally {
        setLoading(false);
      }
    },
    [adminKey, kind, loading, notify]
  );

  const refresh = useCallback(() => {
    void loadPage(1, false);
  }, [loadPage]);

  const loadNext = useCallback(() => {
    if (!loading && hasMore) {
      void loadPage(page + 1, true);
    }
  }, [hasMore, loadPage, loading, page]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    const node = sentinelRef.current;
    if (!node) {
      return undefined;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          loadNext();
        }
      },
      { rootMargin: "240px" }
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [loadNext]);

  const summary = useMemo(() => {
    if (items.length === 0) {
      return "No logs loaded yet.";
    }
    return `Loaded ${items.length} entries${hasMore ? " · auto loading more" : ""}.`;
  }, [hasMore, items.length]);

  return (
    <Panel
      title={title}
      subtitle={subtitle}
      action={
        <button className="btn btn-ghost" type="button" onClick={refresh} disabled={loading}>
          {loading ? "Loading..." : "Refresh"}
        </button>
      }
    >
      <div className="text-xs text-slate-500">{summary}</div>
      {error ? <div className="text-xs text-rose-600">{error}</div> : null}
      {items.length === 0 && !loading ? (
        <div className="text-sm text-slate-500">No records yet.</div>
      ) : (
        <div className="space-y-3">
          {items.map((log) =>
            kind === "upstream" ? (
              <UpstreamCard key={`${kind}-${log.id}`} log={log as UpstreamLog} />
            ) : (
              <DownstreamCard key={`${kind}-${log.id}`} log={log as DownstreamLog} />
            )
          )}
        </div>
      )}
      <div ref={sentinelRef} className="h-8" />
      {hasMore ? (
        <button className="btn btn-ghost w-full" type="button" onClick={loadNext} disabled={loading}>
          {loading ? "Loading..." : "Load more"}
        </button>
      ) : (
        <div className="text-xs text-slate-400">End of log stream.</div>
      )}
    </Panel>
  );
}

export default function LogsSection({ adminKey, notify }: { adminKey: string; notify: Notify }) {
  return (
    <div className="space-y-6">
      <LogFeed
        adminKey={adminKey}
        notify={notify}
        kind="downstream"
        title="Downstream logs"
        subtitle="Incoming requests from users, API keys, and client apps."
      />
      <LogFeed
        adminKey={adminKey}
        notify={notify}
        kind="upstream"
        title="Upstream logs"
        subtitle="Requests sent to providers, including usage and responses."
      />
    </div>
  );
}
