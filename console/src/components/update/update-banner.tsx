import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { DownloadCloud, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { updateCheckQuery, updateStatusQuery } from "@/api/update";
import { instanceSettingsQuery } from "@/api/settings";
import { Button } from "@/components/ui/button";
import { dismissUpdate, readDismissedUpdate } from "@/lib/update-banner-dismissal";
import { ReleaseNotesDialog } from "./release-notes-dialog";

const AUTO_CHECK_STALE_TIME_MS = 15 * 60 * 1000;

/** When enabled in instance settings, checks once when the admin Console opens
 *  and stays quiet unless an update is available. Errors remain on the
 *  dedicated Updates page instead of interrupting normal Console use. */
export function UpdateBanner() {
  const { t } = useTranslation();
  const [dismissedIdentity, setDismissedIdentity] = useState(readDismissedUpdate);
  const [notesOpen, setNotesOpen] = useState(false);
  const { data: settings = [] } = useQuery(instanceSettingsQuery);
  const autoCheckEnabled = settings[0]?.enable_auto_update_check === true;
  const status = useQuery({ ...updateStatusQuery, enabled: autoCheckEnabled });
  const { data } = useQuery({
    ...updateCheckQuery,
    enabled: autoCheckEnabled
      && (status.isError || (status.data !== undefined && status.data.state !== "unavailable")),
    staleTime: AUTO_CHECK_STALE_TIME_MS,
  });

  if (!autoCheckEnabled) return null;
  if (status.data?.state === "unavailable") return null;
  if (!data?.available || dismissedIdentity === data.latest) return null;

  const handleDismiss = () => {
    dismissUpdate(data.latest);
    setDismissedIdentity(data.latest);
  };

  return (
    <>
      <div
        role="status"
        aria-live="polite"
        className="border-b border-amber-300 bg-amber-50 text-amber-950 dark:border-amber-800 dark:bg-amber-950 dark:text-amber-100"
      >
        <div className="flex min-h-11 items-center gap-3 px-4 py-2 md:px-6">
          <DownloadCloud className="size-4 shrink-0" aria-hidden />
          <p className="min-w-0 flex-1 text-sm font-medium">
            {t("updateBanner.message", { version: data.latest })}
          </p>
          {data.available && data.release_notes_available && (
            <Button size="sm" variant="ghost" className="shrink-0" onClick={() => setNotesOpen(true)}>
              {t("updateBanner.whatsNew")}
            </Button>
          )}
          <Button asChild size="sm" variant="outline" className="shrink-0 bg-background/70">
            <Link to="/update">{t("updateBanner.action")}</Link>
          </Button>
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            className="shrink-0 hover:bg-amber-100 dark:hover:bg-amber-900"
            aria-label={t("actions.close")}
            onClick={handleDismiss}
          >
            <X className="size-4" aria-hidden />
          </Button>
        </div>
      </div>
      {data.available && data.release_notes_available && (
        <ReleaseNotesDialog
          open={notesOpen}
          onOpenChange={setNotesOpen}
          current={data.current}
          latest={data.latest}
        />
      )}
    </>
  );
}
