import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { LoginView } from "../components/LoginView";
import { Nav } from "../components/Nav";
import { Toast, type ToastState } from "../components/Toast";
import { Button } from "../components/ui";
import { APP_BUILD_INFO } from "../lib/build-info";
import { apiJson } from "../lib/api";
import { authHeaders } from "../lib/auth";
import type { UpdateCheckResponse } from "../lib/types/admin";
import { I18nProvider, useI18n } from "./i18n";
import {
  buildAdminNavItems,
  buildUserNavItems,
  defaultModule,
  moduleIdsFor,
  renderActiveModule,
  type UserRole,
} from "./modules";
import { clearSession, loadSession, saveSession, type StoredSession } from "./session";
import { getTheme, setTheme, type Theme } from "./theme";

type LoginResponse = {
  user_id: number;
  session_token: string;
  expires_in_secs: number;
  is_admin: boolean;
};

// Reads the current URL hash and returns a module id valid for this role,
// falling back to the role default. Normalises the hash in the URL when it
// points at an unknown / forbidden module so what we render matches the URL.
function resolveModuleFromHash(role: UserRole): string {
  const fallback = defaultModule(role);
  if (typeof window === "undefined") return fallback;
  const raw = window.location.hash.replace(/^#/, "");
  if (!raw) return fallback;
  if (moduleIdsFor(role).includes(raw)) return raw;
  const { pathname, search } = window.location;
  window.history.replaceState(null, "", `${pathname}${search}`);
  return fallback;
}

function AppFrame() {
  const { locale, setLocale, t } = useI18n();
  const [session, setSession] = useState<StoredSession | null>(null);
  const [role, setRole] = useState<UserRole | null>(null);
  const [activeModule, setActiveModule] = useState("");
  const [loginLoading, setLoginLoading] = useState(false);
  const [restoringSession, setRestoringSession] = useState(true);
  const [theme, setThemeState] = useState<Theme>(() => getTheme());
  const [toast, setToast] = useState<ToastState | null>(null);
  const toastTimerRef = useRef<number | null>(null);

  useEffect(() => {
    setTheme(theme);
  }, [theme]);

  useEffect(() => {
    const restored = loadSession();
    if (restored) {
      const restoredRole: UserRole = restored.isAdmin ? "admin" : "user";
      setSession(restored);
      setRole(restoredRole);
      setActiveModule(resolveModuleFromHash(restoredRole));
    }
    setRestoringSession(false);
  }, []);

  useEffect(() => {
    if (!role) return;
    const onHashChange = () => {
      setActiveModule(resolveModuleFromHash(role));
    };
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  }, [role]);

  const onModuleChange = useCallback((id: string) => {
    setActiveModule(id);
    if (typeof window === "undefined") return;
    const current = window.location.hash.replace(/^#/, "");
    if (current !== id) {
      // Push a history entry so the browser back button steps through
      // previously visited modules within this session.
      window.location.hash = id;
    }
  }, []);

  useEffect(() => {
    return () => {
      if (toastTimerRef.current !== null) {
        window.clearTimeout(toastTimerRef.current);
      }
    };
  }, []);

  const notify = useCallback((kind: ToastState["kind"], message: string) => {
    if (toastTimerRef.current !== null) {
      window.clearTimeout(toastTimerRef.current);
    }
    setToast({ kind, message });
    toastTimerRef.current = window.setTimeout(() => {
      setToast(null);
      toastTimerRef.current = null;
    }, 2600);
  }, []);

  const updateCheckedRef = useRef(false);

  useEffect(() => {
    if (!session || role !== "admin" || updateCheckedRef.current) return;
    updateCheckedRef.current = true;
    apiJson<UpdateCheckResponse>("/admin/update/check", {
      method: "POST",
      headers: authHeaders(session.sessionToken),
    })
      .then((res) => {
        if (res.update_available && res.latest_version) {
          notify("success", t("app.updateAvailable", { version: res.latest_version }));
        }
      })
      .catch(() => {});
  }, [session, role, notify, t]);

  const onLogin = useCallback(
    async (name: string, password: string) => {
      if (!name.trim()) {
        throw new Error(t("app.error.usernameEmpty"));
      }
      if (!password.trim()) {
        throw new Error(t("app.error.passwordEmpty"));
      }

      setLoginLoading(true);
      try {
        const response = await fetch("/login", {
          method: "POST",
          headers: {
            "content-type": "application/json",
          },
          body: JSON.stringify({
            username: name.trim(),
            password,
          }),
        });
        const body = (await response.json().catch(() => null)) as
          | LoginResponse
          | { error?: string }
          | null;
        if (!response.ok) {
          throw new Error(body && "error" in body && body.error ? body.error : `HTTP ${response.status}`);
        }
        if (!body || !("session_token" in body)) {
          throw new Error("invalid login response");
        }
        const nextSession = saveSession(body);
        const nextRole: UserRole = body.is_admin ? "admin" : "user";
        setSession(nextSession);
        setRole(nextRole);
        setActiveModule(resolveModuleFromHash(nextRole));
        notify("success", t("app.loginAs", { role: t(`app.role.${nextRole}`) }));
      } finally {
        setLoginLoading(false);
      }
    },
    [notify, t],
  );

  const onLogout = useCallback(() => {
    clearSession();
    setSession(null);
    setRole(null);
    setActiveModule("");
    if (typeof window !== "undefined" && window.location.hash) {
      const { pathname, search } = window.location;
      window.history.replaceState(null, "", `${pathname}${search}`);
    }
    notify("info", t("app.loggedOut"));
  }, [notify, t]);

  const navItems = useMemo(() => {
    if (!role) {
      return [];
    }
    return role === "admin" ? buildAdminNavItems(t) : buildUserNavItems(t);
  }, [role, t]);

  const content = role && session
    ? renderActiveModule(role, activeModule, t, session.sessionToken, notify)
    : null;

  if (restoringSession) {
    return (
      <div className="loading-shell">
        <p className="text-sm text-muted">{t("app.restoring")}</p>
      </div>
    );
  }

  if (!session || !role) {
    return (
      <div className="app-shell">
        <LoginView onLogin={onLogin} loading={loginLoading} />
        <Toast toast={toast} />
      </div>
    );
  }

  return (
    <div className="app-shell">
      <header className="topbar-shell">
        <div className="topbar-panel mx-auto flex w-full max-w-[1700px] flex-col gap-3 px-4 py-3 md:flex-row md:items-center md:justify-between md:gap-4">
          <div className="flex min-w-0 flex-wrap items-center gap-2 md:gap-3">
            <h1 className="topbar-title">{t("app.title")}</h1>
            <code className="rounded border border-border px-1.5 py-0.5 font-mono text-[11px] text-muted">
              v{APP_BUILD_INFO.version}
            </code>
            <code className="rounded border border-border px-1.5 py-0.5 font-mono text-[11px] text-muted">
              {APP_BUILD_INFO.commit.slice(0, 8)}
            </code>
          </div>
          <div className="flex w-full items-center justify-between gap-2 md:w-auto md:justify-end md:gap-3">
            <button
              type="button"
              className="topbar-locale-toggle topbar-segmented"
              onClick={() => setLocale(locale === "zh" ? "en" : "zh")}
              aria-label={t("app.locale.switcher")}
              title={t("app.locale.switcher")}
            >
              <span
                className={`topbar-segmented-item ${locale === "zh" ? "topbar-segmented-item-active" : ""}`}
              >
                {t("app.locale.short.zh")}
              </span>
              <span
                className={`topbar-segmented-item ${locale === "en" ? "topbar-segmented-item-active" : ""}`}
              >
                {t("app.locale.short.en")}
              </span>
            </button>
            <Button variant="neutral" onClick={() => setThemeState(theme === "dark" ? "light" : "dark")}>
              {theme === "dark" ? t("app.theme.light") : t("app.theme.dark")}
            </Button>
            <Button variant="neutral" onClick={onLogout}>
              {t("app.logout")}
            </Button>
          </div>
        </div>
      </header>
      <main className="layout-shell">
        <Nav items={navItems} active={activeModule} onChange={onModuleChange} />
        <section className="content-shell">{content}</section>
      </main>
      <Toast toast={toast} />
    </div>
  );
}

export function App() {
  return (
    <I18nProvider>
      <AppFrame />
    </I18nProvider>
  );
}
