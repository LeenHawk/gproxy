import { useTranslation } from "react-i18next";
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetDescription } from "@/components/ui/sheet";
import { useMediaQuery, DESKTOP_QUERY } from "@/hooks/use-media-query";
import { RuleSetEditor } from "./rule-set-editor";

export function RuleSetDrawer({
  ruleSetId,
  providerId,
  open,
  onOpenChange,
}: {
  ruleSetId: number | null;
  providerId?: number;
  open: boolean;
  onOpenChange: (o: boolean) => void;
}) {
  const { t } = useTranslation("rules");
  const desktop = useMediaQuery(DESKTOP_QUERY);
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        side={desktop ? "right" : "bottom"}
        className={
          desktop
            ? "w-full overflow-y-auto sm:max-w-xl"
            : "max-h-[92svh] overflow-y-auto rounded-t-lg"
        }
      >
        <SheetHeader>
          <SheetTitle>{t("rule.list")}</SheetTitle>
          <SheetDescription className="sr-only">{t("pipeline.caption")}</SheetDescription>
        </SheetHeader>
        <div className="p-4 pt-2">
          {ruleSetId !== null && (
            <RuleSetEditor ruleSetId={ruleSetId} providerId={providerId} />
          )}
        </div>
      </SheetContent>
    </Sheet>
  );
}
