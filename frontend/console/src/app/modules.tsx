import type { NavItem } from "../components/Nav";
import { ConfigExportModule } from "../modules/admin/ConfigExportModule";
import { DashboardModule } from "../modules/admin/dashboard/DashboardModule";
import { FilePermissionsModule } from "../modules/admin/FilePermissionsModule";
import { GlobalSettingsModule } from "../modules/admin/GlobalSettingsModule";
import { PermissionsModule } from "../modules/admin/PermissionsModule";
import { ProvidersModule } from "../modules/admin/ProvidersModule";
import { RateLimitsModule } from "../modules/admin/RateLimitsModule";
import { RequestsModule } from "../modules/admin/RequestsModule";
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

// Single source of truth for valid module ids per role. Derived from the nav
// builders so adding a module automatically makes its id routable.
const STUB_TRANSLATE: TranslateFn = (key) => key;

export function moduleIdsFor(role: UserRole): string[] {
  const items =
    role === "admin" ? buildAdminNavItems(STUB_TRANSLATE) : buildUserNavItems(STUB_TRANSLATE);
  return items.map((item) => item.id);
}

export function buildAdminNavItems(t: TranslateFn): NavItem[] {
  const access = t("app.nav.group.access");
  const operations = t("app.nav.group.operations");
  const account = t("app.nav.group.account");

  return [
    { id: "dashboard", label: t("app.nav.dashboard") },
    { id: "providers", label: t("app.nav.providers") },
    { id: "users", label: t("app.nav.users"), group: access },
    { id: "user-permissions", label: t("app.nav.userPermissions"), group: access },
    { id: "user-file-permissions", label: t("app.nav.userFilePermissions"), group: access },
    { id: "user-rate-limits", label: t("app.nav.userRateLimits"), group: access },
    { id: "global-settings", label: t("app.nav.globalSettings"), group: operations },
    { id: "requests", label: t("app.nav.requests"), group: operations },
    { id: "usages", label: t("app.nav.usages"), group: operations },
    { id: "config-export", label: t("app.nav.configExport"), group: operations },
    { id: "my-keys", label: t("app.nav.myKeys"), group: account },
    { id: "my-quota", label: t("app.nav.myQuota"), group: account },
    { id: "my-usage", label: t("app.nav.myUsage"), group: account },
  ];
}

export function buildUserNavItems(t: TranslateFn): NavItem[] {
  const account = t("app.nav.group.account");
  return [
    { id: "my-keys", label: t("app.nav.myKeys"), group: account },
    { id: "my-quota", label: t("app.nav.myQuota"), group: account },
    { id: "my-usage", label: t("app.nav.myUsage"), group: account },
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
        return <DashboardModule sessionToken={sessionToken} notify={notify} />;
      case "providers":
        return <ProvidersModule sessionToken={sessionToken} notify={notify} />;
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
      case "my-keys":
        return <MyKeysModule sessionToken={sessionToken} notify={notify} />;
      case "my-quota":
        return <MyQuotaModule sessionToken={sessionToken} />;
      case "my-usage":
        return <MyUsageModule sessionToken={sessionToken} notify={notify} />;
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
      return <MyUsageModule sessionToken={sessionToken} notify={notify} />;
    default:
      return null;
  }
}
