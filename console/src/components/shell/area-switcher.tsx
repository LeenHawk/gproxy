import { Link, useRouteContext } from "@tanstack/react-router";
import { ShieldCheck, UserRound } from "lucide-react";
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
        className="inline-flex size-9 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-accent-foreground sm:size-auto sm:gap-1 sm:px-3 sm:py-1.5 sm:text-xs sm:font-medium"
        aria-label={t("nav.myAccount")}
      >
        <UserRound className="size-4" aria-hidden />
        <span className="hidden sm:inline">{t("nav.myAccount")}</span>
      </Link>
    );
  }

  return (
    <Link
      to="/"
      className="inline-flex size-9 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-accent-foreground sm:size-auto sm:gap-1 sm:px-3 sm:py-1.5 sm:text-xs sm:font-medium"
      aria-label={t("nav.adminConsole")}
    >
      <ShieldCheck className="size-4" aria-hidden />
      <span className="hidden sm:inline">{t("nav.adminConsole")}</span>
    </Link>
  );
}
