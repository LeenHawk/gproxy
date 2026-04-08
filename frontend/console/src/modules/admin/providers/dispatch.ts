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
