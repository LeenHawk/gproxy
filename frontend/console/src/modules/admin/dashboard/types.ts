import type {
  CredentialHealthRow,
  DashboardOverview,
  DashboardTopModels,
  DashboardTopProviders,
} from "../../../lib/types/admin";

export type DashboardPreset = "1h" | "24h" | "7d" | "30d";

export type DashboardRange =
  | {
      kind: "preset";
      preset: DashboardPreset;
      fromUnixMs: number;
      toUnixMs: number;
    }
  | {
      kind: "custom";
      preset: null;
      fromUnixMs: number;
      toUnixMs: number;
    };

export type DashboardDataState<T> = {
  data: T;
  loading: boolean;
  error: string | null;
};

export type DashboardBundle = {
  overview: DashboardOverview;
  topProviders: DashboardTopProviders;
  topModels: DashboardTopModels;
  credentialHealth: CredentialHealthRow[];
};
