import type {
  DispatchImplementation,
  DispatchTableDocument,
} from "../../../lib/types/admin";

export type DispatchMode = "Passthrough" | "TransformTo" | "Local" | "Unsupported";

export type DispatchRuleDraft = {
  id: string;
  srcOperation: string;
  srcProtocol: string;
  implementation: DispatchMode;
  destinationOperation: string;
  destinationProtocol: string;
};

export const DISPATCH_OPERATION_OPTIONS = [
  "model_list",
  "model_get",
  "count_tokens",
  "compact",
  "generate_content",
  "stream_generate_content",
  "create_image",
  "stream_create_image",
  "create_image_edit",
  "stream_create_image_edit",
  "openai_response_websocket",
  "gemini_live",
  "embeddings",
  "file_upload",
  "file_list",
  "file_get",
  "file_content",
  "file_delete",
].map((value) => ({ value, label: value }));

export const DISPATCH_PROTOCOL_OPTIONS = [
  "openai",
  "claude",
  "gemini",
  "openai_chat_completions",
  "gemini_ndjson",
  "openai_response",
].map((value) => ({ value, label: value }));

export const DISPATCH_IMPLEMENTATION_OPTIONS = [
  { value: "Passthrough", label: "Passthrough" },
  { value: "TransformTo", label: "TransformTo" },
  { value: "Local", label: "Local" },
  { value: "Unsupported", label: "Unsupported" },
] as const;

function nextDispatchRuleId() {
  return `dispatch-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

export function createDispatchRuleDraft(): DispatchRuleDraft {
  return {
    id: nextDispatchRuleId(),
    srcOperation: "generate_content",
    srcProtocol: "openai",
    implementation: "Passthrough",
    destinationOperation: "",
    destinationProtocol: "",
  };
}

function routeSignature(operation: string, protocol: string) {
  return `${operation.trim().toLowerCase()}::${protocol.trim().toLowerCase()}`;
}

function implementationMode(value: DispatchImplementation): DispatchMode {
  if (value === "Passthrough" || value === "Local" || value === "Unsupported") {
    return value;
  }
  return "TransformTo";
}

export function dispatchDraftsFromDocument(
  document?: DispatchTableDocument | null,
): DispatchRuleDraft[] {
  if (!document || !Array.isArray(document.rules) || document.rules.length === 0) {
    return [createDispatchRuleDraft()];
  }
  return document.rules.map((rule) => ({
    id: nextDispatchRuleId(),
    srcOperation: rule.route.operation,
    srcProtocol: rule.route.protocol,
    implementation: implementationMode(rule.implementation),
    destinationOperation:
      typeof rule.implementation === "object"
        ? rule.implementation.TransformTo.destination.operation
        : "",
    destinationProtocol:
      typeof rule.implementation === "object"
        ? rule.implementation.TransformTo.destination.protocol
        : "",
  }));
}

export function normalizeDispatchDrafts(
  drafts: DispatchRuleDraft[],
): DispatchRuleDraft[] {
  if (drafts.length === 0) {
    throw new Error("dispatch must contain at least one rule");
  }

  const seen = new Set<string>();
  return drafts.map((draft, index) => {
    const srcOperation = draft.srcOperation.trim();
    const srcProtocol = draft.srcProtocol.trim();
    const destinationOperation = draft.destinationOperation.trim();
    const destinationProtocol = draft.destinationProtocol.trim();

    if (!srcOperation || !srcProtocol) {
      throw new Error(`dispatch rule ${index + 1} is missing source route`);
    }

    const signature = routeSignature(srcOperation, srcProtocol);
    if (seen.has(signature)) {
      throw new Error(`dispatch contains duplicate source route ${srcOperation}/${srcProtocol}`);
    }
    seen.add(signature);

    if (
      draft.implementation === "TransformTo" &&
      (!destinationOperation || !destinationProtocol)
    ) {
      throw new Error(`dispatch rule ${index + 1} is missing transform destination`);
    }

    return {
      ...draft,
      srcOperation,
      srcProtocol,
      destinationOperation,
      destinationProtocol,
    };
  });
}

// ---------------------------------------------------------------------------
// Dispatch templates — preset rule sets for common custom channel configs
// ---------------------------------------------------------------------------

export type DispatchTemplate = {
  key: string;
  label: string;
  rules: Array<{
    srcOperation: string;
    srcProtocol: string;
    implementation: DispatchMode;
    destinationOperation: string;
    destinationProtocol: string;
  }>;
};

function pass(op: string, proto: string) {
  return { srcOperation: op, srcProtocol: proto, implementation: "Passthrough" as const, destinationOperation: "", destinationProtocol: "" };
}
function xform(op: string, proto: string, dstOp: string, dstProto: string) {
  return { srcOperation: op, srcProtocol: proto, implementation: "TransformTo" as const, destinationOperation: dstOp, destinationProtocol: dstProto };
}
function local(op: string, proto: string) {
  return { srcOperation: op, srcProtocol: proto, implementation: "Local" as const, destinationOperation: "", destinationProtocol: "" };
}

// Common Local rules for count_tokens across all three base protocols
const LOCAL_COUNT = [
  local("count_tokens", "openai"), local("count_tokens", "claude"), local("count_tokens", "gemini"),
];

// Per-protocol model_list/model_get: passthrough on native, transform others
const OPENAI_MODEL_RULES = [
  pass("model_list", "openai"), xform("model_list", "claude", "model_list", "openai"), xform("model_list", "gemini", "model_list", "openai"),
  pass("model_get", "openai"), xform("model_get", "claude", "model_get", "openai"), xform("model_get", "gemini", "model_get", "openai"),
];
const CLAUDE_MODEL_RULES = [
  pass("model_list", "claude"), xform("model_list", "openai", "model_list", "claude"), xform("model_list", "gemini", "model_list", "claude"),
  pass("model_get", "claude"), xform("model_get", "openai", "model_get", "claude"), xform("model_get", "gemini", "model_get", "claude"),
];
const GEMINI_MODEL_RULES = [
  pass("model_list", "gemini"), xform("model_list", "claude", "model_list", "gemini"), xform("model_list", "openai", "model_list", "gemini"),
  pass("model_get", "gemini"), xform("model_get", "claude", "model_get", "gemini"), xform("model_get", "openai", "model_get", "gemini"),
];

export const DISPATCH_TEMPLATES: DispatchTemplate[] = [
  // --- openai-like (mirrors openai.rs) ---
  {
    key: "openai-like",
    label: "OpenAI-like",
    rules: [
      pass("model_list", "openai"), xform("model_list", "claude", "model_list", "openai"), xform("model_list", "gemini", "model_list", "openai"),
      pass("model_get", "openai"), xform("model_get", "claude", "model_get", "openai"), xform("model_get", "gemini", "model_get", "openai"),
      pass("count_tokens", "openai"), xform("count_tokens", "claude", "count_tokens", "openai"), xform("count_tokens", "gemini", "count_tokens", "openai"),
      pass("generate_content", "openai_response"), pass("generate_content", "openai_chat_completions"),
      xform("generate_content", "claude", "generate_content", "openai_response"),
      xform("generate_content", "gemini", "generate_content", "openai_response"),
      pass("stream_generate_content", "openai_response"), pass("stream_generate_content", "openai_chat_completions"),
      xform("stream_generate_content", "claude", "stream_generate_content", "openai_response"),
      xform("stream_generate_content", "gemini", "stream_generate_content", "openai_response"),
      xform("stream_generate_content", "gemini_ndjson", "stream_generate_content", "openai_response"),
      pass("openai_response_websocket", "openai"),
      xform("gemini_live", "gemini", "stream_generate_content", "openai_response"),
      pass("create_image", "openai"), pass("stream_create_image", "openai"),
      pass("create_image_edit", "openai"), pass("stream_create_image_edit", "openai"),
      pass("embeddings", "openai"), xform("embeddings", "gemini", "embeddings", "openai"),
      pass("compact", "openai"),
    ],
  },
  // --- anthropic-like (mirrors anthropic.rs) ---
  {
    key: "anthropic-like",
    label: "Anthropic-like",
    rules: [
      pass("model_list", "claude"), pass("model_list", "openai"), xform("model_list", "gemini", "model_list", "claude"),
      pass("model_get", "claude"), pass("model_get", "openai"), xform("model_get", "gemini", "model_get", "claude"),
      pass("count_tokens", "claude"), xform("count_tokens", "openai", "count_tokens", "claude"), xform("count_tokens", "gemini", "count_tokens", "claude"),
      pass("generate_content", "claude"), pass("generate_content", "openai_chat_completions"),
      xform("generate_content", "openai_response", "generate_content", "claude"),
      xform("generate_content", "gemini", "generate_content", "claude"),
      pass("stream_generate_content", "claude"), pass("stream_generate_content", "openai_chat_completions"),
      xform("stream_generate_content", "openai_response", "stream_generate_content", "claude"),
      xform("stream_generate_content", "gemini", "stream_generate_content", "claude"),
      xform("stream_generate_content", "gemini_ndjson", "stream_generate_content", "claude"),
      xform("gemini_live", "gemini", "stream_generate_content", "claude"),
      xform("openai_response_websocket", "openai", "stream_generate_content", "claude"),
      xform("compact", "openai", "generate_content", "claude"),
    ],
  },
  // --- gemini-like (mirrors aistudio.rs) ---
  {
    key: "gemini-like",
    label: "Gemini-like",
    rules: [
      pass("model_list", "gemini"), xform("model_list", "claude", "model_list", "gemini"), pass("model_list", "openai"),
      pass("model_get", "gemini"), xform("model_get", "claude", "model_get", "gemini"), pass("model_get", "openai"),
      pass("count_tokens", "gemini"), xform("count_tokens", "claude", "count_tokens", "gemini"), xform("count_tokens", "openai", "count_tokens", "gemini"),
      pass("generate_content", "gemini"), xform("generate_content", "claude", "generate_content", "gemini"),
      pass("generate_content", "openai_chat_completions"), xform("generate_content", "openai_response", "generate_content", "gemini"),
      pass("stream_generate_content", "gemini"), pass("stream_generate_content", "gemini_ndjson"),
      xform("stream_generate_content", "claude", "stream_generate_content", "gemini"),
      pass("stream_generate_content", "openai_chat_completions"),
      xform("stream_generate_content", "openai_response", "stream_generate_content", "gemini"),
      pass("gemini_live", "gemini"), xform("openai_response_websocket", "openai", "gemini_live", "gemini"),
      xform("create_image", "openai", "generate_content", "gemini"),
      xform("stream_create_image", "openai", "stream_generate_content", "gemini"),
      xform("create_image_edit", "openai", "generate_content", "gemini"),
      xform("stream_create_image_edit", "openai", "stream_generate_content", "gemini"),
      pass("embeddings", "gemini"), xform("embeddings", "openai", "embeddings", "gemini"),
      xform("compact", "openai", "generate_content", "gemini"),
    ],
  },
  // --- chat-completions-only ---
  {
    key: "chat-completions-only",
    label: "Chat Completions Only",
    rules: [
      ...OPENAI_MODEL_RULES, ...LOCAL_COUNT,
      pass("generate_content", "openai_chat_completions"),
      pass("stream_generate_content", "openai_chat_completions"),
      xform("generate_content", "openai_response", "generate_content", "openai_chat_completions"),
      xform("stream_generate_content", "openai_response", "stream_generate_content", "openai_chat_completions"),
      xform("generate_content", "claude", "generate_content", "openai_chat_completions"),
      xform("stream_generate_content", "claude", "stream_generate_content", "openai_chat_completions"),
      xform("generate_content", "gemini", "generate_content", "openai_chat_completions"),
      xform("stream_generate_content", "gemini", "stream_generate_content", "openai_chat_completions"),
      xform("stream_generate_content", "gemini_ndjson", "stream_generate_content", "openai_chat_completions"),
      xform("compact", "openai", "generate_content", "openai_chat_completions"),
    ],
  },
  // --- response-only ---
  {
    key: "response-only",
    label: "Response Only",
    rules: [
      ...OPENAI_MODEL_RULES, ...LOCAL_COUNT,
      pass("generate_content", "openai_response"),
      pass("stream_generate_content", "openai_response"),
      pass("compact", "openai"),
      xform("generate_content", "openai_chat_completions", "generate_content", "openai_response"),
      xform("stream_generate_content", "openai_chat_completions", "stream_generate_content", "openai_response"),
      xform("generate_content", "claude", "generate_content", "openai_response"),
      xform("stream_generate_content", "claude", "stream_generate_content", "openai_response"),
      xform("generate_content", "gemini", "generate_content", "openai_response"),
      xform("stream_generate_content", "gemini", "stream_generate_content", "openai_response"),
      xform("stream_generate_content", "gemini_ndjson", "stream_generate_content", "openai_response"),
    ],
  },
  // --- claude-only ---
  {
    key: "claude-only",
    label: "Claude Only",
    rules: [
      ...CLAUDE_MODEL_RULES, ...LOCAL_COUNT,
      pass("generate_content", "claude"),
      pass("stream_generate_content", "claude"),
      xform("generate_content", "openai_chat_completions", "generate_content", "claude"),
      xform("stream_generate_content", "openai_chat_completions", "stream_generate_content", "claude"),
      xform("generate_content", "openai_response", "generate_content", "claude"),
      xform("stream_generate_content", "openai_response", "stream_generate_content", "claude"),
      xform("generate_content", "gemini", "generate_content", "claude"),
      xform("stream_generate_content", "gemini", "stream_generate_content", "claude"),
      xform("stream_generate_content", "gemini_ndjson", "stream_generate_content", "claude"),
      xform("compact", "openai", "generate_content", "claude"),
    ],
  },
  // --- gemini-only ---
  {
    key: "gemini-only",
    label: "Gemini Only",
    rules: [
      ...GEMINI_MODEL_RULES, ...LOCAL_COUNT,
      pass("generate_content", "gemini"),
      pass("stream_generate_content", "gemini"),
      pass("stream_generate_content", "gemini_ndjson"),
      pass("embeddings", "gemini"),
      xform("generate_content", "claude", "generate_content", "gemini"),
      xform("stream_generate_content", "claude", "stream_generate_content", "gemini"),
      xform("generate_content", "openai_chat_completions", "generate_content", "gemini"),
      xform("stream_generate_content", "openai_chat_completions", "stream_generate_content", "gemini"),
      xform("generate_content", "openai_response", "generate_content", "gemini"),
      xform("stream_generate_content", "openai_response", "stream_generate_content", "gemini"),
      xform("compact", "openai", "generate_content", "gemini"),
    ],
  },
];

/** Create a fresh set of DispatchRuleDraft[] from a template (new ids). */
export function applyDispatchTemplate(tmpl: DispatchTemplate): DispatchRuleDraft[] {
  return tmpl.rules.map((r) => ({
    id: `dispatch-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`,
    ...r,
  }));
}

/** Check whether the current dispatch rules match a template exactly. */
export function isDispatchTemplateMatch(
  tmpl: DispatchTemplate,
  current: DispatchRuleDraft[],
): boolean {
  if (current.length !== tmpl.rules.length) return false;
  const sig = (r: { srcOperation: string; srcProtocol: string; implementation: string; destinationOperation: string; destinationProtocol: string }) =>
    `${r.srcOperation}::${r.srcProtocol}::${r.implementation}::${r.destinationOperation}::${r.destinationProtocol}`;
  const currentSigs = new Set(current.map(sig));
  return tmpl.rules.every((r) => currentSigs.has(sig(r)));
}

export function buildDispatchDocument(
  drafts: DispatchRuleDraft[],
): DispatchTableDocument {
  return {
    rules: normalizeDispatchDrafts(drafts).map((draft) => ({
      route: {
        operation: draft.srcOperation,
        protocol: draft.srcProtocol,
      },
      implementation:
        draft.implementation === "TransformTo"
          ? {
              TransformTo: {
                destination: {
                  operation: draft.destinationOperation,
                  protocol: draft.destinationProtocol,
                },
              },
            }
          : draft.implementation,
    })),
  };
}
