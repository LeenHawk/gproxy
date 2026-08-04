import { useMemo } from "react";
import { Link } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { MarkdownContent } from "@/components/update/markdown-content";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { selectNotesSection } from "@/lib/release-notes";

interface ReleaseNotesDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  version: string;
  notes: string;
}

export function ReleaseNotesDialog({ open, onOpenChange, version, notes }: ReleaseNotesDialogProps) {
  const { t, i18n } = useTranslation("update");
  const lines = useMemo(() => {
    const language = i18n.resolvedLanguage ?? i18n.language;
    return selectNotesSection(notes, language);
  }, [i18n.language, i18n.resolvedLanguage, notes]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("releaseNotes.title", { version })}</DialogTitle>
        </DialogHeader>
        <div className="max-h-[60vh] space-y-3 overflow-y-auto pr-2">
          <MarkdownContent markdown={lines} />
        </div>
        <DialogFooter>
          <Button asChild>
            <Link to="/update" onClick={() => onOpenChange(false)}>
              {t("releaseNotes.viewUpdate")}
            </Link>
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
