/* tslint:disable */
/* eslint-disable */
/**
 * The `ReadableStreamType` enum.
 *
 * *This API requires the following crate features to be activated: `ReadableStreamType`*
 */

export type ReadableStreamType = "bytes";

export class IntoUnderlyingByteSource {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    cancel(): void;
    pull(controller: ReadableByteStreamController): Promise<any>;
    start(controller: ReadableByteStreamController): void;
    readonly autoAllocateChunkSize: number;
    readonly type: ReadableStreamType;
}

export class IntoUnderlyingSink {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    abort(reason: any): Promise<any>;
    close(): Promise<any>;
    write(chunk: any): Promise<any>;
}

export class IntoUnderlyingSource {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    cancel(): void;
    pull(controller: ReadableStreamDefaultController): Promise<any>;
}

/**
 * WinterCG fetch entry-point: receives an inbound Request, dispatches it
 * through the same pipeline native uses, directly rather than via axum.
 * Returns 503 if [`super::init`] has not yet been called.
 */
export function gproxyFetch(req: Request): Promise<Response>;

/**
 * Initialise the edge runtime from host-supplied credentials.
 *
 * Persistence is always libSQL/Turso (`turso_url` + `turso_token`). The cache
 * is Upstash Redis when both `upstash_url` and `upstash_token` are non-empty,
 * otherwise it falls back to the libSQL kv table. `master_key` unseals stored
 * secrets (absent → plaintext NoopCipher).
 *
 * Must be called once before [`super::fetch`]. A second call is a no-op (the
 * first `AppState` wins).
 */
export function init(turso_url: string, turso_token: string, upstash_url: string | null | undefined, upstash_token: string | null | undefined, master_key: string | null | undefined, admin_user: string, admin_password: string, log_filter?: string | null): Promise<void>;

/**
 * Edge host hook for downstream Responses WebSocket frames.
 *
 * Platform JS owns the WebSocket upgrade and calls this once per inbound
 * message. Returned array items are JSON text messages to send on the socket.
 */
export function responses_websocket_frame(req: Request, frame: string): Promise<Array<any>>;

/**
 * Run the edge storage self-test against live Turso + Upstash endpoints.
 *
 * Returns a multi-line summary, one line per step (e.g. `libsql.health: OK`,
 * `upstash.incr: 6`, or `libsql.get: ERR <msg>`).
 */
export function storage_selftest(turso_url: string, turso_token: string, upstash_url: string, upstash_token: string): Promise<string>;
