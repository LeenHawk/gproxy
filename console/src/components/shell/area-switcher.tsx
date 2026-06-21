import { Link, useRouteContext } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";

type ShellFrom = "/_app" | "/_portal";

/**
 * Admin-only area switcher in the top bar.
 * - Admin shell (contextFrom="/_app"):     "My Account" → /account/keys
 * - Portal shell (contextFrom="/_portal"): "Admin Console" → /
 * Non-admins see nothing.
 */
export function AreaSwitcher({ contextFrom }: { contextFrom: ShellFrom }) {
  const { t } = useTranslation();
  // strict:false reads from the nearest ancestor — works for both /_app and /_portal
  const ctx = useRouteContext({ strict: false });
  const isAdmin = (ctx as unknown as { user?: { is_admin?: boolean } }).user?.is_admin === true;

  if (!isAdmin) return null;

  if (contextFrom === "/_app") {
    return (
      <Link
        to="/account/keys"
        className="hidden items-center gap-1 rounded-md px-3 py-1.5 text-xs font-medium text-muted-foreground hover:bg-accent hover:text-accent-foreground sm:flex"
        aria-label={t("nav.myAccount")}
      >
        {t("nav.myAccount")}
      </Link>
    );
  }

  return (
    <Link
      to="/"
      className="hidden items-center gap-1 rounded-md px-3 py-1.5 text-xs font-medium text-muted-foreground hover:bg-accent hover:text-accent-foreground sm:flex"
      aria-label={t("nav.adminConsole")}
    >
      {t("nav.adminConsole")}
    </Link>
  );
}
