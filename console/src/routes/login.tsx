import { useEffect, useState } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { useMutation } from "@tanstack/react-query";
import { createFileRoute, isRedirect, redirect, useNavigate, useRouteContext } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { login, sessionQuery } from "@/api/auth";
import { portalSessionQuery } from "@/api/portal";
import { ApiError } from "@/api/http";
import { LocaleControls } from "@/components/locale-controls";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export const Route = createFileRoute("/login")({
  beforeLoad: async ({ context }) => {
    // Admin session: already signed in as admin → go to admin console
    try {
      await context.queryClient.ensureQueryData(sessionQuery);
      throw redirect({ to: "/" });
    } catch (e) {
      if (isRedirect(e)) throw e;
    }
    // Portal session: signed in as non-admin → go to portal
    try {
      await context.queryClient.ensureQueryData(portalSessionQuery);
      throw redirect({ to: "/account/keys" });
    } catch (e) {
      if (isRedirect(e)) throw e;
    }
    // Neither session valid — show login page
  },
  component: LoginPage,
});

const schema = z.object({
  username: z.string().min(1, "required"),
  password: z.string().min(1, "required"),
});
type FormValues = z.infer<typeof schema>;

function LoginPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { queryClient } = useRouteContext({ from: "/login" });
  const [retryIn, setRetryIn] = useState<number | null>(null);

  const form = useForm<FormValues>({
    resolver: zodResolver(schema),
    defaultValues: { username: "", password: "" },
  });

  const mutation = useMutation({
    mutationFn: (values: FormValues) => login(values.username, values.password),
    onSuccess: (data) => {
      queryClient.setQueryData(sessionQuery.queryKey, data.user);
      void navigate({ to: data.user.is_admin ? "/" : "/account/keys" });
    },
    onError: (error) => {
      if (error instanceof ApiError && error.status === 429) {
        setRetryIn(error.retryAfter ?? 60);
      }
    },
  });

  // 429 throttle countdown; clears the stale error when it reaches zero.
  useEffect(() => {
    if (retryIn === null) return;
    if (retryIn <= 0) {
      mutation.reset();
      setRetryIn(null);
      return;
    }
    const timer = setTimeout(() => setRetryIn((s) => (s === null ? null : s - 1)), 1000);
    return () => clearTimeout(timer);
  // mutation.reset is referentially stable; only retryIn drives this effect
  }, [retryIn]);

  const errorText = (() => {
    const error = mutation.error;
    if (!error) return null;
    if (retryIn !== null && retryIn > 0) return t("login.throttled", { seconds: retryIn });
    if (error instanceof ApiError) {
      if (error.status === 401) return t("login.failed");
      if (error.type === "network") return t("errors.network");
    }
    return t("errors.internal");
  })();

  const blocked = (retryIn ?? 0) > 0;

  return (
    <div className="flex min-h-svh flex-col bg-muted/40">
      <div className="flex justify-end p-4">
        <LocaleControls />
      </div>
      <div className="flex flex-1 items-start justify-center px-4 pt-[12svh]">
        <Card className="w-full max-w-sm">
          <CardHeader>
            <CardTitle className="text-lg">{t("login.title")}</CardTitle>
            <p className="text-sm text-muted-foreground">{t("login.subtitle")}</p>
          </CardHeader>
          <CardContent>
            <form
              className="grid gap-4"
              onSubmit={form.handleSubmit((values) => mutation.mutate(values))}
            >
              <div className="grid gap-2">
                <Label htmlFor="username">{t("login.username")}</Label>
                <Input id="username" autoComplete="username" autoFocus {...form.register("username")} />
                {form.formState.errors.username && (
                  <p className="text-xs text-destructive">{t("login.required")}</p>
                )}
              </div>
              <div className="grid gap-2">
                <Label htmlFor="password">{t("login.password")}</Label>
                <Input id="password" type="password" autoComplete="current-password" {...form.register("password")} />
                {form.formState.errors.password && (
                  <p className="text-xs text-destructive">{t("login.required")}</p>
                )}
              </div>
              {errorText && <p className="text-sm text-destructive">{errorText}</p>}
              <Button type="submit" disabled={mutation.isPending || blocked}>
                {t("login.submit")}
              </Button>
            </form>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
