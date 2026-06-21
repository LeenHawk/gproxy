import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { createMyKey, myKeysQuery } from "@/api/portal";
import { ApiError } from "@/api/http";
import { EntityDialog } from "@/components/entity-dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  AlertDialog, AlertDialogContent, AlertDialogHeader, AlertDialogTitle,
  AlertDialogDescription, AlertDialogFooter, AlertDialogCancel,
} from "@/components/ui/alert-dialog";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function MyKeysCreate({ open, onOpenChange }: Props) {
  const { t } = useTranslation("portal");
  const { t: tc } = useTranslation("common");
  const queryClient = useQueryClient();

  const [label, setLabel] = useState("");
  // One-time key reveal state — cleared on dialog close
  const [revealedKey, setRevealedKey] = useState<string | null>(null);
  const [revealOpen, setRevealOpen] = useState(false);
  const [copied, setCopied] = useState(false);

  const generate = useMutation({
    mutationFn: () => createMyKey(label.trim() || null),
    onSuccess: (created) => {
      void queryClient.invalidateQueries({ queryKey: myKeysQuery.queryKey });
      onOpenChange(false);
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

  const handleRevealClose = (isOpen: boolean) => {
    setRevealOpen(isOpen);
    if (!isOpen) {
      // One-time key — clear from state when dialog closes
      setRevealedKey(null);
      setCopied(false);
    }
  };

  return (
    <>
      {/* Create key dialog */}
      <EntityDialog open={open} onOpenChange={onOpenChange} title={t("keys.add")}>
        <form
          className="grid gap-4"
          onSubmit={(e) => { e.preventDefault(); generate.mutate(); }}
        >
          <div className="grid gap-2">
            <Label htmlFor="portal-key-label">{t("keys.label")}</Label>
            <Input
              id="portal-key-label"
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
            <Button
              onClick={() => handleCopy(revealedKey ?? "")}
              disabled={copied}
            >
              {copied ? tc("actions.copied") : tc("actions.copy")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
