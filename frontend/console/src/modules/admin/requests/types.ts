import type {
  DownstreamRequestQueryRow,
  UpstreamRequestQueryRow,
} from "../../../lib/types/admin";

export type RequestKind = "downstream" | "upstream";

/// Discriminated union shared by the filters, table, and state hook so
/// the same row type covers both upstream and downstream queries. The
/// per-kind shape is already enforced by the query type unions in
/// `lib/types/admin.ts`, so we just widen here.
export type RequestRow = DownstreamRequestQueryRow | UpstreamRequestQueryRow;

/// Per-row body payload fetched on demand by `ensureBodyLoaded`. Kept
/// separate from the list row so the expensive `request_body` /
/// `response_body` blobs aren't serialized into the table-level state.
export type RequestBodyPayload = {
  request_body: number[] | null;
  response_body: number[] | null;
};

/// Form state for the filters panel. Values are always strings so the
/// inputs can stay controlled; `useRequestsModuleState` parses them into
/// a strongly typed snapshot when the user hits "Query".
export type RequestsFilterState = {
  providerId: string;
  credentialId: string;
  userId: string;
  userKeyId: string;
  requestPathContains: string;
  fromAt: string;
  toAt: string;
  limit: string;
};

/// Immutable snapshot of the filters at the moment a query was issued.
/// Stored in `activeQuery` so pagination re-queries use the exact same
/// filter set as the initial fetch (the form values can drift while the
/// user scrolls).
export type RequestQuerySnapshot = {
  kind: RequestKind;
  providerId: number | null;
  credentialId: number | null;
  userId: number | null;
  userKeyId: number | null;
  pathContains: string;
  fromUnixMs: number | null;
  toUnixMs: number | null;
  maxRows: number | null;
};

export type SelectOption = { value: string; label: string };

export type NotifyFn = (kind: "success" | "error" | "info", message: string) => void;
export type TranslateFn = (key: string, params?: Record<string, string | number>) => string;
