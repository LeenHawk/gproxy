import type { DispatchRuleDraft } from "./dispatch";

export type ProviderWorkspaceTab = "config" | "credentials" | "oauth";

export type ProviderFormState = {
  id: string;
  name: string;
  channel: string;
  settings: Record<string, string>;
  dispatchRules: DispatchRuleDraft[];
};

export type CredentialFormState = {
  values: Record<string, string>;
  editingIndex: number | null;
};
