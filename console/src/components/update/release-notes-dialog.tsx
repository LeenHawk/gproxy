import { useMemo } from "react";
import { Link } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { parseReleaseNotes, selectNotesSection, type ReleaseNoteLine } from "@/lib/release-notes";

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
    return parseReleaseNotes(selectNotesSection(notes, language));
  }, [i18n.language, i18n.resolvedLanguage, notes]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("releaseNotes.title", { version })}</DialogTitle>
        </DialogHeader>
        <div className="max-h-[60vh] space-y-3 overflow-y-auto pr-2">
          {lines.map((line, index) => <ReleaseNotesRow key={index} line={line} />)}
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

function ReleaseNotesRow({ line }: { line: ReleaseNoteLine }) {
  if (line.kind === "heading") {
    if (line.level === 2) return <h2 className="text-lg font-semibold">{line.content}</h2>;
    if (line.level === 3) return <h3 className="text-base font-semibold">{line.content}</h3>;
    return <h4 className="pt-1 font-semibold">{line.content}</h4>;
  }
  if (line.kind === "blockquote") {
    return <blockquote className="border-l-2 pl-3 text-muted-foreground">{line.content}</blockquote>;
  }
  if (line.kind === "bullet") {
    return <div className="flex gap-2"><span aria-hidden>•</span><p>{line.content}</p></div>;
  }
  return <p className="leading-relaxed">{line.content}</p>;
}
