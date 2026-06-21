import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  userKeysQuery, createUserKey, deleteUserKey, type UserView, type UserKeyView,
} from "@/api/identity";
import { ApiError } from "@/api/http";
import { ConfirmDangerous } from "@/components/confirm-dangerous";
import { DataTable, type DataColumn } from "@/components/data-table";
import { EntityDialog } from "@/components/entity-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import {
  AlertDialog, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogCancel,
} from "@/components/ui/alert-dialog";

export function UserKeysTab({ user }: { user: UserView }) {
  const { t } = useTranslation("identity");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();
  const { data: keys, isPending } = useQuery(userKeysQuery(user.id));

  const [generateOpen, setGenerateOpen] = useState(false);
  const [label, setLabel] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<UserKeyView | undefined>(undefined);
  // One-time key reveal state — cleared on dialog close
  const [revealedKey, setRevealedKey] = useState<string | null>(null);
  const [revealOpen, setRevealOpen] = useState(false);
  const [copied, setCopied] = useState(false);

  const generate = useMutation({
    mutationFn: () => createUserKey(user.id, { label: label.trim() || null, enabled: true }),
    onSuccess: (created) => {
      void queryClient.invalidateQueries({ queryKey: userKeysQuery(user.id).queryKey });
      setGenerateOpen(false);
      setLabel("");
      if (created.api_key != null) {
        setRevealedKey(created.api_key);
        setCopied(false);
        setRevealOpen(true);
      }
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
    },
  });

  const removal = useMutation({
    mutationFn: (id: number) => deleteUserKey(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: userKeysQuery(user.id).queryKey });
      toast.success(tc("actions.deleted"));
      setDeleteTarget(undefined);
    },
    onError: (error) => {
      toast.error(error instanceof ApiError ? error.message : String(error));
      setDeleteTarget(undefined);
    },
  });

  const handleCopy = async (value: string) => {
    if (!navigator.clipboard) { toast.error(t("keys.copyFailed")); return; }
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      toast.success(t("keys.copied"));
    } catch {
      toast.error(t("keys.copyFailed"));
    }
  };

  const handleRevealClose = (open: boolean) => {
    setRevealOpen(open);
    if (!open) {
      // Clear the one-time key from state when dialog closes
      setRevealedKey(null);
      setCopied(false);
    }
  };

  const actions = (k: UserKeyView) => (
    <div className="flex items-center justify-end">
      <Button
        variant="ghost"
        size="icon"
        className="text-destructive"
        aria-label={t("keys.delete")}
        onClick={(e) => { e.stopPropagation(); setDeleteTarget(k); }}
      >
        <Trash2 className="size-4" aria-hidden />
      </Button>
    </div>
  );

  const columns: DataColumn<UserKeyView>[] = [
    {
      key: "label",
      header: t("keys.label"),
      cell: (k) => <span className="text-sm">{k.label ?? "—"}</span>,
    },
    {
      key: "key_prefix",
      header: t("keys.prefix"),
      cell: (k) => <span className="font-mono text-sm">{k.key_prefix}</span>,
    },
    {
      key: "enabled",
      header: t("keys.enabled"),
      cell: (k) => <Badge variant={k.enabled ? "secondary" : "outline"}>{k.enabled ? "on" : "off"}</Badge>,
    },
    { key: "actions", header: "", cell: actions, className: "w-16 text-right" },
  ];

  return (
    <div className="grid gap-3">
      <div className="flex items-center justify-between">
        <p className="text-xs text-muted-foreground">{t("keys.rotateHint")}</p>
        <Button onClick={() => { setLabel(""); setGenerateOpen(true); }}>
          <Plus className="size-4" aria-hidden />
          {t("keys.add")}
        </Button>
      </div>

      {isPending ? (
        <div className="grid gap-2" aria-busy="true">
          <Skeleton className="h-10" /><Skeleton className="h-10" />
        </div>
      ) : (
        <DataTable
          columns={columns}
          rows={keys ?? []}
          rowKey={(k) => k.id}
          empty={t("keys.empty")}
          renderCard={(k) => (
            <div className="grid gap-2">
              <div className="flex items-center justify-between">
                <span className="text-sm">{k.label ?? "—"}</span>
                <Badge variant={k.enabled ? "secondary" : "outline"}>{k.enabled ? "on" : "off"}</Badge>
              </div>
              <span className="font-mono text-xs text-muted-foreground">{k.key_prefix}</span>
              {actions(k)}
            </div>
          )}
        />
      )}

      {/* Generate key dialog */}
      <EntityDialog open={generateOpen} onOpenChange={setGenerateOpen} title={t("keys.add")}>
        <form
          className="grid gap-4"
          onSubmit={(e) => { e.preventDefault(); generate.mutate(); }}
        >
          <div className="grid gap-2">
            <Label htmlFor="key-label">{t("keys.label")}</Label>
            <Input
              id="key-label"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder={t("keys.label")}
            />
          </div>
          <Button type="submit" disabled={generate.isPending}>{t("keys.add")}</Button>
        </form>
      </EntityDialog>

      {/* One-time key reveal */}
      <AlertDialog open={revealOpen} onOpenChange={handleRevealClose}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("keys.title")}</AlertDialogTitle>
            <AlertDialogDescription>{t("keys.created")}</AlertDialogDescription>
          </AlertDialogHeader>
          <div className="rounded-md border bg-muted px-3 py-2">
            <code className="break-all font-mono text-sm">{revealedKey}</code>
          </div>
          <AlertDialogFooter>
            <AlertDialogCancel>{tc("actions.close")}</AlertDialogCancel>
            <Button onClick={() => handleCopy(revealedKey ?? "")} disabled={copied}>
              {copied ? tc("actions.copied") : tc("actions.copy")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Delete confirmation */}
      <ConfirmDangerous
        open={deleteTarget !== undefined}
        onOpenChange={(o) => { if (!o) setDeleteTarget(undefined); }}
        title={t("keys.delete")}
        description={t("keys.deleteConfirm")}
        confirmLabel={t("keys.delete")}
        onConfirm={() => { if (deleteTarget) removal.mutate(deleteTarget.id); }}
        pending={removal.isPending}
      />
    </div>
  );
}
