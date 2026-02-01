import React, { useActionState, useCallback, useEffect, useState } from "react";
import Sidebar, { type NavItem } from "./components/Sidebar";
import Topbar from "./components/Topbar";
import { Toast, type ToastState } from "./components/Toast";
import { apiRequest } from "./lib/api";
import { maskValue } from "./lib/format";
import OverviewSection from "./sections/OverviewSection";
import ChannelsSection from "./sections/ChannelsSection";
import DisallowSection from "./sections/DisallowSection";
import UsersSection from "./sections/UsersSection";
import KeysSection from "./sections/KeysSection";
import StatsSection from "./sections/StatsSection";
import UsageSection from "./sections/UsageSection";
import LogsSection from "./sections/LogsSection";
import ConfigSection from "./sections/ConfigSection";
import AboutSection from "./sections/AboutSection";

const NAV_ITEMS: NavItem[] = [
  { id: "overview", label: "Overview", description: "Health & snapshots" },
  { id: "channels", label: "Channels", description: "Per-provider credentials" },
  { id: "disallow", label: "Disallow", description: "Cooldowns & bans" },
  { id: "users", label: "Users", description: "API principals" },
  { id: "keys", label: "Keys", description: "Access tokens" },
  { id: "stats", label: "Stats", description: "Pool coverage" },
  { id: "usage", label: "Usage", description: "Token analytics" },
  { id: "logs", label: "Logs", description: "Upstream & downstream" },
  { id: "config", label: "Config", description: "Runtime settings" },
  { id: "about", label: "About", description: "Panel info" }
];

type AuthState = {
  status: "idle" | "success" | "error";
  message?: string;
  key?: string;
};

export default function App() {
  const [adminKey, setAdminKey] = useState(() => localStorage.getItem("gproxy_admin_key") || "");
  const [authed, setAuthed] = useState(false);
  const [toast, setToast] = useState<ToastState>(null);
  const [active, setActive] = useState<string>(NAV_ITEMS[0].id);

  useEffect(() => {
    if (!toast) {
      return;
    }
    const timer = setTimeout(() => setToast(null), 3200);
    return () => clearTimeout(timer);
  }, [toast]);

  const notify = useCallback((next: Exclude<ToastState, null>) => {
    setToast(next);
  }, []);

  const validateKey = useCallback(
    async (key: string) => {
      try {
        await apiRequest("/admin/health", { adminKey: key });
        localStorage.setItem("gproxy_admin_key", key);
        setAdminKey(key);
        setAuthed(true);
        notify({ type: "success", message: "Authenticated." });
        return true;
      } catch (error) {
        notify({ type: "error", message: "Authentication failed." });
        setAuthed(false);
        return false;
      }
    },
    [notify]
  );

  const [authState, loginAction, isPending] = useActionState<
    AuthState,
    FormData
  >(async (_prev, formData) => {
    const key = String(formData.get("adminKey") || "").trim();
    if (!key) {
      return { status: "error", message: "Admin key is required." };
    }
    const ok = await validateKey(key);
    return ok
      ? { status: "success", key }
      : { status: "error", message: "Authentication failed." };
  }, { status: "idle" });

  useEffect(() => {
    if (adminKey) {
      void validateKey(adminKey);
    }
  }, [adminKey, validateKey]);

  const logout = () => {
    localStorage.removeItem("gproxy_admin_key");
    setAdminKey("");
    setAuthed(false);
  };

  const renderSection = () => {
    switch (active) {
      case "overview":
        return <OverviewSection adminKey={adminKey} notify={notify} />;
      case "channels":
        return <ChannelsSection adminKey={adminKey} notify={notify} />;
      case "disallow":
        return <DisallowSection adminKey={adminKey} notify={notify} />;
      case "users":
        return <UsersSection adminKey={adminKey} notify={notify} />;
      case "keys":
        return <KeysSection adminKey={adminKey} notify={notify} />;
      case "stats":
        return <StatsSection adminKey={adminKey} notify={notify} />;
      case "usage":
        return <UsageSection adminKey={adminKey} notify={notify} />;
      case "logs":
        return <LogsSection adminKey={adminKey} notify={notify} />;
      case "config":
        return (
          <ConfigSection
            adminKey={adminKey}
            notify={notify}
            onAdminKeyUpdate={(next) => {
              localStorage.setItem("gproxy_admin_key", next);
              setAdminKey(next);
              notify({ type: "info", message: "Admin key updated." });
            }}
          />
        );
      case "about":
        return <AboutSection />;
      default:
        return null;
    }
  };

  if (!authed) {
    return (
      <div className="flex min-h-screen items-center justify-center px-6 py-12">
        <div className="card card-shadow w-full max-w-md p-8">
          <div className="text-sm uppercase tracking-[0.3em] text-slate-400">gproxy</div>
          <div className="mt-2 text-2xl font-semibold text-slate-900">Admin Console</div>
          <div className="mt-2 text-sm text-slate-500">
            Enter your admin key to unlock the control plane.
          </div>
          <form action={loginAction} className="mt-6 space-y-4">
            <div>
              <label className="label">Admin key</label>
              <input
                className="input"
                name="adminKey"
                type="password"
                placeholder="••••••••"
                autoComplete="current-password"
              />
            </div>
            <button className="btn btn-primary w-full" type="submit" disabled={isPending}>
              {isPending ? "Validating..." : "Sign in"}
            </button>
          </form>
        </div>
        <Toast toast={toast} />
      </div>
    );
  }

  const activeItem = NAV_ITEMS.find((item) => item.id === active);

  return (
    <div className="min-h-screen px-6 py-8">
      <Toast toast={toast} />
      <div className="mx-auto flex max-w-6xl flex-col gap-6 lg:flex-row">
        <Sidebar items={NAV_ITEMS} active={active} onChange={setActive} />
        <main className="flex-1 space-y-6">
          <Topbar
            title={activeItem?.label ?? "Admin"}
            subtitle={activeItem?.description}
            actions={
              <div className="flex flex-wrap items-center gap-3">
                <span className="rounded-full border border-slate-200 bg-white/80 px-3 py-1 text-xs text-slate-500">
                  Key {maskValue(adminKey, 5, 3)}
                </span>
                <button className="btn btn-primary" type="button" onClick={logout}>
                  Sign out
                </button>
              </div>
            }
          />
          {renderSection()}
        </main>
      </div>
    </div>
  );
}
