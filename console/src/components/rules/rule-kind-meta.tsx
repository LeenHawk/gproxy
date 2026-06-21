import { Type, Database, Pencil, Eraser, Heading } from "lucide-react";
import type { LucideIcon } from "lucide-react";

export const RULE_KIND_META: Record<string, { icon: LucideIcon; descKey: string }> = {
  system_text: { icon: Type, descKey: "kindDesc.system_text" },
  cache_breakpoint: { icon: Database, descKey: "kindDesc.cache_breakpoint" },
  rewrite: { icon: Pencil, descKey: "kindDesc.rewrite" },
  sanitize: { icon: Eraser, descKey: "kindDesc.sanitize" },
  header: { icon: Heading, descKey: "kindDesc.header" },
};

/** One-line human summary of a rule's config for list/pipeline rows. */
export function summarizeRuleConfig(kind: string, config: unknown): string {
  const c = (config ?? {}) as Record<string, unknown>;
  switch (kind) {
    case "system_text":
      return `${c.position ?? "prepend"}: ${String(c.text ?? "").slice(0, 40)}`;
    case "cache_breakpoint":
      return `${c.target ?? "system"}${c.ttl ? ` · ${c.ttl}` : ""}`;
    case "rewrite":
      return `${c.action ?? "set"} ${c.path ?? ""}`;
    case "sanitize":
      return `/${c.pattern ?? ""}/ → ${c.replacement ?? ""}`;
    case "header":
      return `${c.name ?? ""}: ${c.value ?? ""}`;
    default:
      return JSON.stringify(config).slice(0, 50);
  }
}
