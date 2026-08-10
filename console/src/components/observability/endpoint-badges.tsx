import { Badge } from "@/components/ui/badge";

const KIND_LABELS: Record<string, string> = {
  open_ai_chat_completions: "chat",
  open_ai_responses: "responses",
  open_ai_responses_websocket: "responses-ws",
  claude_messages: "claude",
  gemini_generate_content: "gemini",
  open_ai: "openai",
  claude: "claude",
  gemini: "gemini",
};

const OPERATION_LABELS: Record<string, string> = {
  list_models: "models",
  get_model: "model",
  count_tokens: "count",
  create_image: "images",
  edit_image: "image-edit",
  create_embedding: "embeddings",
  rerank: "rerank",
  compact_content: "compact",
  create_conversation: "conversation",
  connect_realtime: "realtime",
};

const CONTENT_OPERATIONS = new Set(["generate_content", "stream_generate_content"]);

interface EndpointBadgesProps {
  kind: string;
  operation: string;
}

export function EndpointBadges({ kind, operation }: EndpointBadgesProps) {
  return (
    <div className="flex flex-wrap items-center gap-1">
      <Badge variant="secondary" className="h-5 px-1.5 font-mono text-[10px]">
        {KIND_LABELS[kind] ?? kind.replaceAll("_", "-")}
      </Badge>
      {!CONTENT_OPERATIONS.has(operation) && (
        <Badge variant="outline" className="h-5 px-1.5 font-mono text-[10px]">
          {OPERATION_LABELS[operation] ?? operation.replaceAll("_", "-")}
        </Badge>
      )}
    </div>
  );
}
