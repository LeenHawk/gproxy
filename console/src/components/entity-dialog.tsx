import type { ReactNode } from "react";
import { DESKTOP_QUERY, useMediaQuery } from "@/hooks/use-media-query";
import {
  Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle,
} from "@/components/ui/dialog";
import {
  Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle,
} from "@/components/ui/sheet";
import { cn } from "@/lib/utils";

interface EntityDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  wide?: boolean;
  workspace?: boolean;
  scrollClassName?: string;
}

/** Form-hosting modal: Dialog on md+, bottom Sheet on mobile (spec §4).
 *  Crossing the 768px boundary while open remounts the children — keep all form
 *  state lifted/controlled, never in uncontrolled inputs. */
export function EntityDialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  wide,
  workspace,
  scrollClassName,
}: EntityDialogProps) {
  const desktop = useMediaQuery(DESKTOP_QUERY);
  if (desktop) {
    return (
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent className={workspace
          ? "sm:max-w-[calc(100vw-3rem)] xl:max-w-6xl"
          : wide ? "sm:max-w-2xl" : "sm:max-w-lg"}
        >
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
            {description ? <DialogDescription>{description}</DialogDescription> : null}
          </DialogHeader>
          {/* min-w-0: a fixed-width child (e.g. a recharts svg mid-measure) must
              not inflate this grid track past the dialog and clip siblings. */}
          <div className={cn("max-h-[70svh] min-w-0 overflow-y-auto pr-1", scrollClassName)}>
            {children}
          </div>
        </DialogContent>
      </Dialog>
    );
  }
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        side="bottom"
        className={cn("max-h-[92svh] overflow-y-auto rounded-t-lg p-4", scrollClassName)}
      >
        <SheetHeader className="p-0 pb-3 text-left">
          <SheetTitle>{title}</SheetTitle>
          {description ? <SheetDescription>{description}</SheetDescription> : null}
        </SheetHeader>
        {children}
      </SheetContent>
    </Sheet>
  );
}
