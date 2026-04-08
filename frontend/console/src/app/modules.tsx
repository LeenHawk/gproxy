import { Card } from "../components/ui";
import type { NavItem } from "../components/Nav";

export type UserRole = "admin" | "user";

type TranslateFn = (key: string, params?: Record<string, string | number>) => string;

function PlaceholderModule({
  title,
  description,
}: {
  title: string;
  description: string;
}) {
  return (
    <Card title={title}>
      <p className="text-sm text-muted">{description}</p>
    </Card>
  );
}

export function defaultModule(role: UserRole) {
  return role === "admin" ? "dashboard" : "my-quota";
}

export function buildAdminNavItems(t: TranslateFn): NavItem[] {
  return [
    { id: "dashboard", label: t("app.nav.dashboard") },
    { id: "global-settings", label: t("app.nav.globalSettings") },
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

export function renderActiveModule(role: UserRole, activeModule: string, t: TranslateFn) {
  if (role === "admin") {
    switch (activeModule) {
      case "dashboard":
        return (
          <PlaceholderModule
            title={t("placeholder.dashboard.title")}
            description={t("placeholder.description")}
          />
        );
      case "global-settings":
        return (
          <PlaceholderModule
            title={t("placeholder.globalSettings.title")}
            description={t("placeholder.description")}
          />
        );
      case "my-keys":
        return (
          <PlaceholderModule
            title={t("placeholder.myKeys.title")}
            description={t("placeholder.description")}
          />
        );
      case "my-quota":
        return (
          <PlaceholderModule
            title={t("placeholder.myQuota.title")}
            description={t("placeholder.description")}
          />
        );
      case "my-usage":
        return (
          <PlaceholderModule
            title={t("placeholder.myUsage.title")}
            description={t("placeholder.description")}
          />
        );
      default:
        return null;
    }
  }

  switch (activeModule) {
    case "my-keys":
      return (
        <PlaceholderModule
          title={t("placeholder.myKeys.title")}
          description={t("placeholder.description")}
        />
      );
    case "my-quota":
      return (
        <PlaceholderModule
          title={t("placeholder.myQuota.title")}
          description={t("placeholder.description")}
        />
      );
    case "my-usage":
      return (
        <PlaceholderModule
          title={t("placeholder.myUsage.title")}
          description={t("placeholder.description")}
        />
      );
    default:
      return null;
  }
}
