import { Plus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@/components/ui/select";
import { channelMeta, type EndpointKind } from "@/lib/channel-meta";

export interface EndpointRow {
  kind: EndpointKind | "";
  url: string;
}

interface EndpointFieldsProps {
  channel: string;
  rows: EndpointRow[];
  onChange: (rows: EndpointRow[]) => void;
}

export function isValidEndpointUrl(input: string): boolean {
  const value = input.trim();
  const placeholder = /\{(?:model|organization)\}/g;
  const authorityStart = value.indexOf("://") + 3;
  const pathStart = value.indexOf("/", authorityStart);
  const queryStart = value.search(/[?#]/);
  const pathEnd = queryStart === -1 ? value.length : queryStart;

  for (const match of value.matchAll(placeholder)) {
    if (pathStart === -1 || match.index < pathStart || match.index >= pathEnd) return false;
  }

  const normalized = value.replace(placeholder, "placeholder");
  if (/[{}]/.test(normalized)) return false;
  try {
    const url = new URL(normalized);
    return (url.protocol === "http:" || url.protocol === "https:") && url.hostname !== "";
  } catch {
    return false;
  }
}

export function EndpointFields({ channel, rows, onChange }: EndpointFieldsProps) {
  const { t } = useTranslation("providers");
  const kinds = channelMeta(channel)?.endpointKinds ?? [];
  if (kinds.length === 0) return null;

  const selected = new Set(rows.map((row) => row.kind).filter(Boolean));
  const canAdd = !rows.some((row) => !row.kind) && kinds.some((kind) => !selected.has(kind));
  const update = (index: number, next: Partial<EndpointRow>) =>
    onChange(rows.map((row, i) => i === index ? { ...row, ...next } : row));

  return (
    <div className="grid gap-2">
      <div className="flex items-center justify-between gap-3">
        <Label>{t("endpoints.title")}</Label>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={!canAdd}
          onClick={() => onChange([...rows, { kind: "", url: "" }])}
        >
          <Plus />
          {t("endpoints.add")}
        </Button>
      </div>
      <p className="text-xs text-muted-foreground">{t("endpoints.hint")}</p>
      {rows.map((row, index) => (
        <Card key={`${row.kind}-${index}`} size="sm">
          <CardContent className="grid gap-2 sm:grid-cols-[minmax(0,13rem)_minmax(0,1fr)_auto]">
            <Select
              value={row.kind}
              onValueChange={(kind) => update(index, { kind: kind as EndpointKind })}
            >
              <SelectTrigger className="w-full" aria-label={t("endpoints.kind")}>
                <SelectValue placeholder={t("endpoints.kindPlaceholder")} />
              </SelectTrigger>
              <SelectContent>
                {kinds.map((kind) => (
                  <SelectItem
                    key={kind}
                    value={kind}
                    disabled={kind !== row.kind && selected.has(kind)}
                  >
                    {t(`endpoints.kinds.${kind}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Input
              value={row.url}
              onChange={(event) => update(index, { url: event.target.value })}
              placeholder={t("endpoints.urlPlaceholder")}
              aria-label={t("endpoints.url")}
            />
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label={t("endpoints.remove")}
              onClick={() => onChange(rows.filter((_, i) => i !== index))}
            >
              <Trash2 />
            </Button>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}
