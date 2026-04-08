import type { NavItem } from "../components/Nav";
import { ConfigExportModule } from "../modules/admin/ConfigExportModule";
import { DashboardModule } from "../modules/admin/DashboardModule";
import { FilePermissionsModule } from "../modules/admin/FilePermissionsModule";
import { GlobalSettingsModule } from "../modules/admin/GlobalSettingsModule";
import { ModelAliasesModule } from "../modules/admin/ModelAliasesModule";
import { ModelsModule } from "../modules/admin/ModelsModule";
import { PermissionsModule } from "../modules/admin/PermissionsModule";
import { ProvidersModule } from "../modules/admin/ProvidersModule";
import { RateLimitsModule } from "../modules/admin/RateLimitsModule";
import { RequestsModule } from "../modules/admin/RequestsModule";
import { UpdateModule } from "../modules/admin/UpdateModule";
import { UsageModule } from "../modules/admin/UsageModule";
import { UsersModule } from "../modules/admin/UsersModule";
import { MyKeysModule } from "../modules/user/MyKeysModule";
import { MyQuotaModule } from "../modules/user/MyQuotaModule";
import { MyUsageModule } from "../modules/user/MyUsageModule";

export type UserRole = "admin" | "user";

type TranslateFn = (key: string, params?: Record<string, string | number>) => string;

export function defaultModule(role: UserRole) {
  return role === "admin" ? "dashboard" : "my-quota";
}

export function buildAdminNavItems(t: TranslateFn): NavItem[] {
  return [
    { id: "dashboard", label: t("app.nav.dashboard") },
    { id: "providers", label: t("app.nav.providers") },
    { id: "models", label: t("app.nav.models") },
    { id: "model-aliases", label: t("app.nav.modelAliases") },
    { id: "users", label: t("app.nav.users") },
    { id: "user-permissions", label: t("app.nav.userPermissions") },
    { id: "user-file-permissions", label: t("app.nav.userFilePermissions") },
    { id: "user-rate-limits", label: t("app.nav.userRateLimits") },
    { id: "global-settings", label: t("app.nav.globalSettings") },
    { id: "requests", label: t("app.nav.requests") },
    { id: "usages", label: t("app.nav.usages") },
    { id: "config-export", label: t("app.nav.configExport") },
    { id: "update", label: t("app.nav.update") },
    { id: "my-keys", label: t("app.nav.myKeys") },
    { id: "my-quota", label: t("app.nav.myQuota") },
    { id: "my-usage", label: t("app.nav.myUsage") },
  ];
}

export function buildUserNavItems(t: TranslateFn): NavItem[] {
  return [
    { id: "my-keys", label: t("app.nav.myKeys") },
    { id: "my-quota", label: t("app.nav.myQuota") },
    { id: "my-usage", label: t("app.nav.myUsage") },
  ];
}

export function renderActiveModule(
  role: UserRole,
  activeModule: string,
  _t: TranslateFn,
  sessionToken: string,
  notify: (kind: "success" | "error" | "info", message: string) => void,
) {
  if (role === "admin") {
    switch (activeModule) {
      case "dashboard":
        return <DashboardModule sessionToken={sessionToken} />;
      case "providers":
        return <ProvidersModule sessionToken={sessionToken} notify={notify} />;
      case "models":
        return <ModelsModule sessionToken={sessionToken} notify={notify} />;
      case "model-aliases":
        return <ModelAliasesModule sessionToken={sessionToken} notify={notify} />;
      case "users":
        return <UsersModule sessionToken={sessionToken} notify={notify} />;
      case "user-permissions":
        return <PermissionsModule sessionToken={sessionToken} notify={notify} />;
      case "user-file-permissions":
        return <FilePermissionsModule sessionToken={sessionToken} notify={notify} />;
      case "user-rate-limits":
        return <RateLimitsModule sessionToken={sessionToken} notify={notify} />;
      case "global-settings":
        return <GlobalSettingsModule sessionToken={sessionToken} notify={notify} />;
      case "requests":
        return <RequestsModule sessionToken={sessionToken} notify={notify} />;
      case "usages":
        return <UsageModule sessionToken={sessionToken} notify={notify} />;
      case "config-export":
        return <ConfigExportModule sessionToken={sessionToken} notify={notify} />;
      case "update":
        return <UpdateModule sessionToken={sessionToken} notify={notify} />;
      case "my-keys":
        return <MyKeysModule sessionToken={sessionToken} notify={notify} />;
      case "my-quota":
        return <MyQuotaModule sessionToken={sessionToken} />;
      case "my-usage":
        return <MyUsageModule sessionToken={sessionToken} />;
      default:
        return null;
    }
  }

  switch (activeModule) {
    case "my-keys":
      return <MyKeysModule sessionToken={sessionToken} notify={notify} />;
    case "my-quota":
      return <MyQuotaModule sessionToken={sessionToken} />;
    case "my-usage":
      return <MyUsageModule sessionToken={sessionToken} />;
    default:
      return null;
  }
}
