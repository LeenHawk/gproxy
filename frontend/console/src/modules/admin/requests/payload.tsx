import { useState, type ReactNode } from "react";

import { copyText } from "../../../lib/clipboard";
import type {
  DownstreamRequestQueryRow,
  UpstreamRequestQueryRow,
} from "../../../lib/types/admin";
import type {
  NotifyFn,
  RequestBodyPayload,
  RequestRow,
  TranslateFn,
} from "./types";

const META_DEFAULT_PREVIEW_CHARS = 420;

type PayloadPreview = {
  preview: string;
  full: string;
  truncated: boolean;
};

function EyeToggleIcon({ open }: { open: boolean }) {
  return (
    <svg
      viewBox="0 0 24 24"
      aria-hidden="true"
      className="h-4 w-4"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M2 12s3.5-6 10-6 10 6 10 6-3.5 6-10 6-10-6-10-6z" />
      {open ? <circle cx="12" cy="12" r="2.5" /> : <path d="M4 20L20 4" />}
    </svg>
  );
}

function BodyEyeButton({
  ariaLabel,
  open,
  loading,
  onClick,
}: {
  ariaLabel: string;
  open: boolean;
  loading: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="inline-flex cursor-pointer items-center text-muted hover:text-text disabled:cursor-not-allowed disabled:opacity-60"
      onClick={onClick}
      aria-label={ariaLabel}
      disabled={loading}
    >
      <EyeToggleIcon open={open} />
    </button>
  );
}

function BodyCopyButton({
  ariaLabel,
  loading,
  onClick,
}: {
  ariaLabel: string;
  loading: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="relative z-10 inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-border bg-panel-muted text-muted transition hover:text-text disabled:cursor-not-allowed disabled:opacity-60"
      onClick={onClick}
      aria-label={ariaLabel}
      title={ariaLabel}
      disabled={loading}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.8"
        className="h-4 w-4"
        aria-hidden="true"
      >
        <rect x="9" y="9" width="11" height="11" rx="2" />
        <path d="M6 15H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v1" />
      </svg>
    </button>
  );
}

/// One labeled payload section (request query, headers, body, etc.).
/// Supports inline toggling between truncated preview and full content when
/// the source text exceeds the preview budget.
function PayloadSection({
  title,
  section,
  action,
}: {
  title: string;
  section: PayloadPreview;
  action?: ReactNode;
}) {
  const [showFull, setShowFull] = useState(false);
  if (!section.preview) {
    return null;
  }
  const canToggle = section.truncated && section.full !== section.preview;
  const content = showFull ? section.full : section.preview;

  return (
    <div>
      <div className="mb-1 flex items-center gap-1 font-semibold text-muted">
        <span>{title}</span>
        {action}
        {canToggle ? (
          <button
            type="button"
            className="inline-flex cursor-pointer items-center text-muted hover:text-text"
            aria-label={`${showFull ? "show truncated" : "show full"} ${title}`}
            onClick={() => setShowFull((value) => !value)}
          >
            <EyeToggleIcon open={showFull} />
          </button>
        ) : null}
      </div>
      <pre className="whitespace-pre-wrap break-all rounded px-2 py-1">{content}</pre>
    </div>
  );
}

function bytesToUtf8Preview(bytes: number[] | null): PayloadPreview {
  if (!bytes || bytes.length === 0) {
    return { preview: "", full: "", truncated: false };
  }
  try {
    const decoded = new TextDecoder().decode(new Uint8Array(bytes));
    return { preview: decoded, full: decoded, truncated: false };
  } catch {
    const binary = `[binary ${bytes.length} bytes]`;
    return { preview: binary, full: binary, truncated: false };
  }
}

function textToPreview(text: string | null | undefined): PayloadPreview {
  if (!text) {
    return { preview: "", full: "", truncated: false };
  }
  if (text.length <= META_DEFAULT_PREVIEW_CHARS) {
    return { preview: text, full: text, truncated: false };
  }
  return {
    preview: text.slice(0, META_DEFAULT_PREVIEW_CHARS),
    full: text,
    truncated: true,
  };
}

function jsonToPreview(value: Record<string, unknown>): PayloadPreview {
  const text = JSON.stringify(value);
  if (!text || text === "{}") {
    return { preview: "", full: "", truncated: false };
  }
  return textToPreview(text);
}

function normalizeRequestQuery(query: string | null | undefined): string | null {
  if (!query) {
    return null;
  }
  const trimmed = query.trim();
  if (!trimmed) {
    return null;
  }
  const normalized = trimmed.startsWith("?") ? trimmed.slice(1) : trimmed;
  return normalized.trim() ? normalized.trim() : null;
}

function extractQueryFromRequestUrl(requestUrl: string | null | undefined): string | null {
  if (!requestUrl) {
    return null;
  }
  const trimmed = requestUrl.trim();
  if (!trimmed) {
    return null;
  }
  try {
    const parsed =
      trimmed.startsWith("http://") || trimmed.startsWith("https://")
        ? new URL(trimmed)
        : new URL(trimmed, "http://localhost");
    return normalizeRequestQuery(parsed.search);
  } catch {
    const index = trimmed.indexOf("?");
    if (index < 0) {
      return null;
    }
    return normalizeRequestQuery(trimmed.slice(index));
  }
}

function requestQueryFromRow(row: RequestRow): string | null {
  if ("request_query" in row) {
    return normalizeRequestQuery((row as DownstreamRequestQueryRow).request_query);
  }
  return extractQueryFromRequestUrl((row as UpstreamRequestQueryRow).request_url);
}

async function copyOrNotify(
  text: string,
  notify: NotifyFn,
  t: TranslateFn,
): Promise<void> {
  if (!text) {
    notify("info", t("common.none"));
    return;
  }
  try {
    await copyText(text);
    notify("success", t("common.copied"));
  } catch {
    notify("error", t("common.copyFailed"));
  }
}

/// Collapsible payload cell for each request row. Mirrors the sample gproxy
/// admin UI: a `<details>` summary containing request query / headers / body
/// and response headers / body. Body bytes are only fetched when the user
/// toggles the eye icon, so the list query can stay lightweight
/// (`include_body = false`).
export function PayloadCell({
  row,
  t,
  notify,
  detail,
  loadingBody,
  bodyError,
  ensureBodyLoaded,
}: {
  row: RequestRow;
  t: TranslateFn;
  notify: NotifyFn;
  detail: RequestBodyPayload | undefined;
  loadingBody: boolean;
  bodyError: string | undefined;
  ensureBodyLoaded: (row: RequestRow) => Promise<RequestBodyPayload | undefined>;
}) {
  const [showReqBody, setShowReqBody] = useState(false);
  const [showRespBody, setShowRespBody] = useState(false);
  const requestHeaders = jsonToPreview(row.request_headers_json);
  const responseHeaders = jsonToPreview(row.response_headers_json);
  const requestQuery = textToPreview(requestQueryFromRow(row));
  const requestBody = bytesToUtf8Preview(detail?.request_body ?? null);
  const responseBody = bytesToUtf8Preview(detail?.response_body ?? null);
  const emptyStub: PayloadPreview = { preview: "-", full: "-", truncated: false };
  const reqBodySection = showReqBody && requestBody.preview ? requestBody : emptyStub;
  const respBodySection = showRespBody && responseBody.preview ? responseBody : emptyStub;

  const toggleReqBody = () => {
    if (!showReqBody && !detail && !loadingBody) {
      void ensureBodyLoaded(row);
    }
    setShowReqBody((value) => !value);
  };

  const toggleRespBody = () => {
    if (!showRespBody && !detail && !loadingBody) {
      void ensureBodyLoaded(row);
    }
    setShowRespBody((value) => !value);
  };

  const copyReqBody = async () => {
    let loadedDetail = detail;
    if (!loadedDetail && !loadingBody) {
      loadedDetail = await ensureBodyLoaded(row);
    }
    await copyOrNotify(bytesToUtf8Preview(loadedDetail?.request_body ?? null).full, notify, t);
  };

  const copyRespBody = async () => {
    let loadedDetail = detail;
    if (!loadedDetail && !loadingBody) {
      loadedDetail = await ensureBodyLoaded(row);
    }
    await copyOrNotify(bytesToUtf8Preview(loadedDetail?.response_body ?? null).full, notify, t);
  };

  return (
    <details className="payload-cell">
      <summary className="cursor-pointer text-xs text-muted" aria-label="toggle payload" />
      <div className="mt-2 space-y-2 text-xs">
        {requestQuery.preview ? (
          <PayloadSection title="req query" section={requestQuery} />
        ) : (
          <div>
            <div className="mb-1 font-semibold text-muted">req query</div>
            <div className="text-xs text-muted">-</div>
          </div>
        )}
        <PayloadSection
          title="req headers"
          section={requestHeaders}
          action={
            <BodyCopyButton
              ariaLabel={t("common.copy")}
              loading={false}
              onClick={() => void copyOrNotify(requestHeaders.full, notify, t)}
            />
          }
        />
        <PayloadSection
          title="req body"
          section={reqBodySection}
          action={
            <div className="inline-flex items-center gap-1">
              <BodyEyeButton
                ariaLabel="toggle req body"
                open={showReqBody}
                loading={loadingBody}
                onClick={toggleReqBody}
              />
              <BodyCopyButton
                ariaLabel={t("common.copy")}
                loading={loadingBody}
                onClick={() => void copyReqBody()}
              />
            </div>
          }
        />
        {responseHeaders.preview ? (
          <PayloadSection
            title="resp headers"
            section={responseHeaders}
            action={
              <BodyCopyButton
                ariaLabel={t("common.copy")}
                loading={false}
                onClick={() => void copyOrNotify(responseHeaders.full, notify, t)}
              />
            }
          />
        ) : (
          <div>
            <div className="mb-1 flex items-center gap-1 font-semibold text-muted">
              <span>resp headers</span>
            </div>
            <div className="text-xs text-muted">-</div>
          </div>
        )}
        <PayloadSection
          title="resp body"
          section={respBodySection}
          action={
            <div className="inline-flex items-center gap-1">
              <BodyEyeButton
                ariaLabel="toggle resp body"
                open={showRespBody}
                loading={loadingBody}
                onClick={toggleRespBody}
              />
              <BodyCopyButton
                ariaLabel={t("common.copy")}
                loading={loadingBody}
                onClick={() => void copyRespBody()}
              />
            </div>
          }
        />
        {loadingBody ? <div className="text-xs text-muted">{t("common.loading")}</div> : null}
        {bodyError ? <div className="text-xs text-amber-700">{bodyError}</div> : null}
      </div>
    </details>
  );
}
