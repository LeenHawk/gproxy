import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ChevronRight, Loader2, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { releaseNotesQuery, type ReleaseNotesEntry } from "@/api/update";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { MarkdownContent } from "@/components/update/markdown-content";
import {
  selectNotesSection,
  sortReleaseNotesDescending,
} from "@/lib/release-notes";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

interface ReleaseNotesDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  current: string;
  latest: string;
}

export function ReleaseNotesDialog({
  open,
  onOpenChange,
  current,
  latest,
}: ReleaseNotesDialogProps) {
  const { t, i18n } = useTranslation("update");
  const notes = useQuery({
    ...releaseNotesQuery(current, latest),
    enabled: open,
  });
  const entries = useMemo(
    () => sortReleaseNotesDescending(notes.data?.entries ?? []),
    [notes.data?.entries],
  );
  const language = i18n.resolvedLanguage ?? i18n.language;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {t("releaseNotes.title", {
              current: notes.data?.current ?? current,
              latest: notes.data?.latest ?? latest,
            })}
          </DialogTitle>
        </DialogHeader>

        <div className="max-h-[60vh] overflow-y-auto pr-2">
          {(notes.isPending || (notes.isFetching && !notes.data)) && (
            <div
              role="status"
              aria-live="polite"
              className="flex min-h-32 items-center justify-center gap-2 text-muted-foreground"
            >
              <Loader2 className="size-4 animate-spin" aria-hidden />
              {t("releaseNotes.loading")}
            </div>
          )}

          {notes.isError && !notes.data && (
            <NotesUnavailable retrying={notes.isFetching} onRetry={() => { void notes.refetch(); }} />
          )}

          {notes.data && (
            <div className="grid gap-3">
              {!notes.data.complete && entries.length > 0 && (
                <div
                  role="status"
                  className="flex items-start gap-2 rounded-md border border-amber-500/50 bg-amber-50 p-3 text-amber-900 dark:bg-amber-950 dark:text-amber-200"
                >
                  <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
                  <div className="grid flex-1 gap-2">
                    <p>{t("releaseNotes.incomplete")}</p>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      className="w-fit bg-background/70"
                      disabled={notes.isFetching}
                      onClick={() => { void notes.refetch(); }}
                    >
                      {notes.isFetching && <Loader2 className="mr-2 size-4 animate-spin" aria-hidden />}
                      {t("releaseNotes.retry")}
                    </Button>
                  </div>
                </div>
              )}

              {entries.length === 0 ? (
                <NotesUnavailable retrying={notes.isFetching} onRetry={() => { void notes.refetch(); }} />
              ) : entries.map((entry, index) => (
                <ReleaseVersionNotes
                  key={entry.version}
                  entry={entry}
                  language={language}
                  defaultOpen={index === 0}
                />
              ))}
            </div>
          )}
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

function ReleaseVersionNotes({
  entry,
  language,
  defaultOpen,
}: {
  entry: ReleaseNotesEntry;
  language: string;
  defaultOpen: boolean;
}) {
  const markdown = useMemo(
    () => selectNotesSection(entry.body, language),
    [entry.body, language],
  );
  const version = entry.version.startsWith("v") ? entry.version : `v${entry.version}`;

  return (
    <Collapsible defaultOpen={defaultOpen} className="rounded-lg border">
      <CollapsibleTrigger className="group flex w-full items-center gap-2 px-4 py-3 text-left font-mono font-semibold hover:bg-muted/50">
        <ChevronRight
          className="size-4 shrink-0 transition-transform group-data-[state=open]:rotate-90"
          aria-hidden
        />
        {version}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <MarkdownContent markdown={markdown} className="border-t px-4 py-3" />
      </CollapsibleContent>
    </Collapsible>
  );
}

function NotesUnavailable({ retrying, onRetry }: { retrying: boolean; onRetry: () => void }) {
  const { t } = useTranslation("update");
  return (
    <div role="alert" className="grid min-h-32 place-content-center justify-items-center gap-3 text-center">
      <p className="text-muted-foreground">{t("releaseNotes.unavailable")}</p>
      <Button type="button" size="sm" variant="outline" disabled={retrying} onClick={onRetry}>
        {retrying && <Loader2 className="mr-2 size-4 animate-spin" aria-hidden />}
        {t("releaseNotes.retry")}
      </Button>
    </div>
  );
}
