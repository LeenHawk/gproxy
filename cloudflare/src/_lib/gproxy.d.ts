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

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly gproxyFetch: (a: number) => number;
    readonly init: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number) => number;
    readonly responses_websocket_frame: (a: number, b: number, c: number) => number;
    readonly storage_selftest: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => number;
    readonly __wbg_intounderlyingbytesource_free: (a: number, b: number) => void;
    readonly __wbg_intounderlyingsink_free: (a: number, b: number) => void;
    readonly __wbg_intounderlyingsource_free: (a: number, b: number) => void;
    readonly intounderlyingbytesource_autoAllocateChunkSize: (a: number) => number;
    readonly intounderlyingbytesource_cancel: (a: number) => void;
    readonly intounderlyingbytesource_pull: (a: number, b: number) => number;
    readonly intounderlyingbytesource_start: (a: number, b: number) => void;
    readonly intounderlyingbytesource_type: (a: number) => number;
    readonly intounderlyingsink_abort: (a: number, b: number) => number;
    readonly intounderlyingsink_close: (a: number) => number;
    readonly intounderlyingsink_write: (a: number, b: number) => number;
    readonly intounderlyingsource_cancel: (a: number) => void;
    readonly intounderlyingsource_pull: (a: number, b: number) => number;
    readonly __wasm_bindgen_func_elem_13644: (a: number, b: number, c: number, d: number) => void;
    readonly __wasm_bindgen_func_elem_13705: (a: number, b: number, c: number, d: number) => void;
    readonly __wasm_bindgen_func_elem_16883: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number) => void;
    readonly __wbindgen_export4: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export5: (a: number, b: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
