import type { RoutingRuleDraft } from "./routing";

export type ProviderWorkspaceTab = "config" | "credentials" | "models" | "rewrite" | "oauth";

export type ProviderFormState = {
  id: string;
  name: string;
  label: string;
  channel: string;
  settings: Record<string, string>;
  routingRules: RoutingRuleDraft[];
};

export type CredentialFormState = {
  values: Record<string, string>;
  editingIndex: number | null;
  rawJson: string;
};
