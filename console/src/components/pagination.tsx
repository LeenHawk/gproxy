import { ChevronLeft, ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";

export type PaginationItem = number | "ellipsis";

export function getPaginationItems(page: number, totalPages: number): PaginationItem[] {
  if (totalPages <= 7) {
    return Array.from({ length: totalPages }, (_, index) => index + 1);
  }
  if (page <= 4) return [1, 2, 3, 4, "ellipsis", totalPages - 1, totalPages];
  if (page >= totalPages - 3) {
    return [1, 2, "ellipsis", totalPages - 3, totalPages - 2, totalPages - 1, totalPages];
  }
  return [1, 2, "ellipsis", page - 1, page, page + 1, "ellipsis", totalPages - 1, totalPages];
}

interface PaginationProps {
  page: number;
  totalPages: number;
  onPageChange: (page: number) => void;
  disabled?: boolean;
}

export function Pagination({ page, totalPages, onPageChange, disabled = false }: PaginationProps) {
  const { t } = useTranslation("common");
  if (totalPages <= 1) return null;

  return (
    <nav aria-label={t("pagination.label")} className="flex flex-wrap items-center justify-center gap-1 pt-2">
      <Button
        type="button"
        variant="outline"
        size="icon-sm"
        aria-label={t("pagination.previous")}
        disabled={disabled || page <= 1}
        onClick={() => onPageChange(page - 1)}
      >
        <ChevronLeft aria-hidden />
      </Button>
      {getPaginationItems(page, totalPages).map((item, index) =>
        item === "ellipsis" ? (
          <span key={`ellipsis-${index}`} aria-hidden className="px-1 text-sm text-muted-foreground">…</span>
        ) : (
          <Button
            key={item}
            type="button"
            variant={item === page ? "default" : "outline"}
            size="icon-sm"
            aria-label={t("pagination.page", { page: item })}
            aria-current={item === page ? "page" : undefined}
            disabled={disabled}
            onClick={() => onPageChange(item)}
          >
            {item}
          </Button>
        ),
      )}
      <Button
        type="button"
        variant="outline"
        size="icon-sm"
        aria-label={t("pagination.next")}
        disabled={disabled || page >= totalPages}
        onClick={() => onPageChange(page + 1)}
      >
        <ChevronRight aria-hidden />
      </Button>
    </nav>
  );
}
