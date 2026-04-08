export type ProviderWorkspaceTab = "config" | "credentials" | "status" | "oauth" | "usage";

export type ProviderFormState = {
  id: string;
  name: string;
  channel: string;
  settings: Record<string, string>;
  dispatchJson: string;
};

export type CredentialFormState = {
  values: Record<string, string>;
  editingIndex: number | null;
};
