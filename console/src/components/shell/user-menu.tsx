import { LogOut } from "lucide-react";
import { useMutation } from "@tanstack/react-query";
import type { QueryClient } from "@tanstack/react-query";
import { useNavigate, useRouteContext } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { logout } from "@/api/auth";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

export function UserMenu() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  // strict:false reads from the nearest ancestor route context — works for both /_app and /_portal
  const ctx = useRouteContext({ strict: false });
  const { queryClient, user } = ctx as unknown as {
    queryClient: QueryClient;
    user: { name: string; is_admin: boolean };
  };

  const mutation = useMutation({
    mutationFn: logout,
    onSuccess: () => {
      queryClient.clear();
      void navigate({ to: "/login" });
    },
    onError: () => {
      toast.error(t("user.logoutFailed"));
    },
  });

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon" className="rounded-full" aria-label={user.name || t("user.menu")}>
          <Avatar className="size-8">
            <AvatarFallback>{(user.name || "?").slice(0, 2).toUpperCase()}</AvatarFallback>
          </Avatar>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-48">
        <DropdownMenuLabel className="font-normal">
          <p className="text-xs text-muted-foreground">{t("user.signedInAs")}</p>
          <p className="truncate text-sm font-medium">{user.name}</p>
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={() => mutation.mutate()} disabled={mutation.isPending}>
          <LogOut className="size-4" />
          {t("user.logout")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
