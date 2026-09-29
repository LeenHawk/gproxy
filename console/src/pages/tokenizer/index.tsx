import { useState, type FormEvent } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTranslation } from "react-i18next"
import { Download, Link2, Star, Trash2 } from "lucide-react"
import { toast } from "sonner"
import { SETTINGS_KEY, readSettings, saveSettings, vocabularies } from "@/api/settings"
import {
  MODEL_CATALOG_KEY,
  VOCABULARIES_KEY,
  bindVocabulary,
  deleteVocabulary,
  downloadVocabulary,
  modelCatalog,
  vocabularyProgress,
} from "@/api/tokenizer"
import type { ModelDto, TokenizerFetch, VocabularyDto } from "@/generated/sdk"
import { Page, PageHeader } from "@/components/page"
import { DataTable } from "@/components/data-table"
import { InstantCell } from "@/components/cells"
import { ConfirmButton } from "@/components/confirm"
import { EmptyNotice, ErrorNotice, QueryState } from "@/components/state"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import { Checkbox } from "@/components/ui/checkbox"
import { DownloadProgress } from "@/components/download-progress"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Link } from "@/lib/router"

function size(bytes: number) {
  return bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KiB` : `${(bytes / 1024 / 1024).toFixed(1)} MiB`
}

export function TokenizerPage() {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [downloading, setDownloading] = useState(false)
  const [binding, setBinding] = useState<VocabularyDto | null>(null)
  const [search, setSearch] = useState("")
  const list = useQuery({ queryKey: VOCABULARIES_KEY, queryFn: vocabularies })
  const settings = useQuery({ queryKey: SETTINGS_KEY, queryFn: readSettings })
  const models = useQuery({ queryKey: MODEL_CATALOG_KEY, queryFn: modelCatalog })
  const invalidate = async () => {
    await Promise.all([
      client.invalidateQueries({ queryKey: VOCABULARIES_KEY }),
      client.invalidateQueries({ queryKey: SETTINGS_KEY }),
      client.invalidateQueries({ queryKey: MODEL_CATALOG_KEY }),
    ])
  }
  const changeDefault = useMutation({
    mutationFn: (fileId: string | null) => saveSettings({ instance: { defaultVocabularyFileId: fileId } }),
    onSuccess: async () => {
      await invalidate()
      toast.success(t("toast.saved"))
    },
  })
  const remove = useMutation({
    mutationFn: deleteVocabulary,
    onSuccess: async () => {
      await invalidate()
      toast.success(t("toast.deleted"))
    },
  })
  const fetch = useMutation({
    mutationFn: downloadVocabulary,
    onMutate: async () => {
      await client.cancelQueries({ queryKey: ["tokenizer", "progress"] })
      client.setQueryData(["tokenizer", "progress"], null)
    },
    onSuccess: async () => {
      setDownloading(false)
      await invalidate()
      toast.success(t("tokenizer.downloaded"))
    },
  })
  const progress = useQuery({
    queryKey: ["tokenizer", "progress"],
    queryFn: vocabularyProgress,
    enabled: fetch.isPending,
    refetchInterval: fetch.isPending ? 500 : false,
    staleTime: 0,
  })
  const rows = (list.data ?? []).filter((row) =>
    `${row.filename ?? ""} ${row.fileId}`.toLowerCase().includes(search.toLowerCase()),
  )
  const busy = fetch.isPending || remove.isPending || changeDefault.isPending
  return (
    <Page>
      <PageHeader
        title={t("nav.tokenizer")}
        actions={
          <Button
            size="sm"
            disabled={fetch.isPending || !settings.data?.instance.enableTokenizerDownload}
            onClick={() => {
              fetch.reset()
              setDownloading(true)
            }}
          >
            <Download data-icon="inline-start" />
            {t("tokenizer.download")}
          </Button>
        }
      />
      <div className="flex flex-wrap items-center gap-3">
        <Input
          className="max-w-xs"
          aria-label={t("actions.search")}
          placeholder={t("actions.search")}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        {settings.data ? (
          <Badge variant={settings.data.instance.enableTokenizerVocabs ? "success" : "outline"}>
            {settings.data.instance.enableTokenizerVocabs ? t("tokenizer.enabled") : t("tokenizer.disabled")}
          </Badge>
        ) : null}
        <Link to="/settings" className="text-sm underline underline-offset-4">
          {t("nav.settings")}
        </Link>
      </div>
      {settings.data && !settings.data.instance.enableTokenizerDownload ? (
        <p role="status" className="text-sm">
          {t("tokenizer.downloadDisabled")}
        </p>
      ) : null}
      {changeDefault.error || remove.error || settings.error || models.error ? (
        <ErrorNotice error={changeDefault.error ?? remove.error ?? settings.error ?? models.error} />
      ) : null}
      <QueryState isPending={list.isPending} error={list.error}>
        <DataTable storageKey="tokenizers" paginate resetPageKey={search}
          rows={rows}
          rowKey={(row) => row.fileId}
          empty={<EmptyNotice title={t("tokenizer.empty")} />}
          columns={[
            {
              key: "filename",
              cell: (row) => (
                <span className="inline-flex items-center gap-2">
                  {row.filename ?? row.fileId}
                  {row.isDefault ? <Badge variant="secondary">{t("tokenizer.default")}</Badge> : null}
                </span>
              ),
            },
            { key: "sizeBytes", cell: (row) => size(row.sizeBytes) },
            {
              key: "linkedModels",
              cell: (row) =>
                row.models
                  .map((id) => models.data?.find((model) => model.id === id)?.name ?? id)
                  .join(", ") || "—",
            },
            { key: "createdAtMs", cell: (row) => <InstantCell value={row.createdAtMs} /> },
          ]}
          actions={(row) => (
            <>
              <Button
                variant="ghost"
                size="sm"
                disabled={busy}
                onClick={() => changeDefault.mutate(row.isDefault ? null : row.fileId)}
              >
                <Star data-icon="inline-start" />
                {row.isDefault ? t("tokenizer.unsetDefault") : t("tokenizer.setDefault")}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                disabled={busy || models.isPending || !!models.error}
                onClick={() => setBinding(row)}
              >
                <Link2 data-icon="inline-start" />
                {t("tokenizer.bind")}
              </Button>
              <ConfirmButton
                disabled={busy}
                title={t("tokenizer.delete", { name: row.filename ?? row.fileId })}
                onConfirm={() => remove.mutate(row.fileId)}
              >
                <Trash2 data-icon="inline-start" />
                {t("actions.delete")}
              </ConfirmButton>
            </>
          )}
        />
      </QueryState>
      <Dialog
        open={downloading}
        onOpenChange={(open) => {
          if (!fetch.isPending) setDownloading(open)
        }}
      >
        <DialogContent
          className="sm:max-w-lg"
          closeLabel={t("actions.close")}
          showCloseButton={!fetch.isPending}
          aria-describedby={undefined}
        >
          <DialogHeader>
            <DialogTitle>{t("tokenizer.download")}</DialogTitle>
          </DialogHeader>
          {downloading ? (
            <DownloadForm
              models={models.data ?? []}
              pending={fetch.isPending}
              error={fetch.error}
              onSubmit={(request) => fetch.mutate(request)}
              progress={progress.data?.repo === fetch.variables?.repo && progress.data?.filename === (fetch.variables?.filename || "tokenizer.json") ? progress.data : null}
              progressError={progress.error}
            />
          ) : null}
        </DialogContent>
      </Dialog>
      <Dialog
        open={binding !== null}
        onOpenChange={(open) => {
          if (!open) setBinding(null)
        }}
      >
        <DialogContent className="sm:max-w-lg" closeLabel={t("actions.close")} aria-describedby={undefined}>
          <DialogHeader>
            <DialogTitle>{t("tokenizer.bind")}</DialogTitle>
          </DialogHeader>
          {binding ? (
            <ModelBindings
              key={binding.fileId}
              vocabulary={binding}
              models={models.data ?? []}
              onSaved={async () => {
                setBinding(null)
                await invalidate()
                toast.success(t("toast.saved"))
              }}
            />
          ) : null}
        </DialogContent>
      </Dialog>
    </Page>
  )
}

function DownloadForm({
  models,
  pending,
  error,
  onSubmit,
  progress,
  progressError,
}: {
  models: Array<ModelDto>
  pending: boolean
  error: unknown
  onSubmit: (request: TokenizerFetch) => void
  progress: import("@/generated/sdk").TokenizerProgressDto | null | undefined
  progressError: unknown
}) {
  const { t } = useTranslation()
  const [repo, setRepo] = useState("")
  const [filename, setFilename] = useState("tokenizer.json")
  const [model, setModel] = useState("__none")
  const [asDefault, setAsDefault] = useState(false)
  const submit = (e: FormEvent) => {
    e.preventDefault()
    onSubmit({
      repo: repo.trim(),
      filename: filename.trim() || null,
      modelId: model === "__none" ? null : model,
      setAsDefault: asDefault,
    })
  }
  return (
    <form onSubmit={submit} className="flex min-h-0 flex-col">
      <DialogBody>
        {error ? <ErrorNotice error={error} /> : null}
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="vocab-repo">{t("tokenizer.repo")}</FieldLabel>
            <Input
              id="vocab-repo"
              required
              placeholder="owner/model"
              value={repo}
              disabled={pending}
              onChange={(e) => setRepo(e.target.value)}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="vocab-file">{t("fields.filename")}</FieldLabel>
            <Input
              id="vocab-file"
              value={filename}
              disabled={pending}
              onChange={(e) => setFilename(e.target.value)}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="vocab-model">{t("tokenizer.bind")}</FieldLabel>
            <Select value={model} onValueChange={setModel} disabled={pending}>
              <SelectTrigger id="vocab-model" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="__none">{t("tokenizer.noBinding")}</SelectItem>
                  {models.map((m) => (
                    <SelectItem key={m.id} value={m.id}>
                      {m.name}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>
          <Field>
            <FieldLabel htmlFor="vocab-default">{t("tokenizer.setDefault")}</FieldLabel>
            <Switch
              id="vocab-default"
              checked={asDefault}
              disabled={pending}
              onCheckedChange={setAsDefault}
            />
          </Field>
        </FieldGroup>
        {pending ? (
          <div role="status" className="mt-5 flex flex-col gap-2">
            <DownloadProgress
              label={t("tokenizer.downloading")}
              downloaded={progress?.downloadedBytes}
              total={progress?.totalBytes}
            />
            {progressError ? <ErrorNotice error={progressError} /> : null}
          </div>
        ) : null}
      </DialogBody>
      <DialogFooter>
        <Button type="submit" disabled={pending}>
          <Download data-icon="inline-start" />
          {pending ? t("tokenizer.downloading") : t("tokenizer.download")}
        </Button>
      </DialogFooter>
    </form>
  )
}

function ModelBindings({
  vocabulary,
  models,
  onSaved,
}: {
  vocabulary: VocabularyDto
  models: Array<ModelDto>
  onSaved: () => Promise<void>
}) {
  const { t } = useTranslation()
  const [selected, setSelected] = useState(new Set(vocabulary.models))
  const saved = useMutation({
    mutationFn: async () => {
      for (const model of models) {
        const wanted = selected.has(model.id)
        if (wanted && model.vocabularyFileId !== vocabulary.fileId)
          await bindVocabulary(model.id, vocabulary.fileId)
        else if (!wanted && model.vocabularyFileId === vocabulary.fileId) await bindVocabulary(model.id, null)
      }
    },
    onSuccess: onSaved,
  })
  return (
    <>
      <DialogBody>
        {saved.error ? <ErrorNotice error={saved.error} /> : null}
        {models.length === 0 ? (
          <EmptyNotice title={t("tokenizer.noModels")} />
        ) : (
          <div className="flex flex-col gap-3">
            {models.map((model) => (
              <label key={model.id} className="flex items-center gap-3">
                <Checkbox
                  disabled={saved.isPending}
                  checked={selected.has(model.id)}
                  onCheckedChange={(checked) =>
                    setSelected((current) => {
                      const next = new Set(current)
                      if (checked) next.add(model.id)
                      else next.delete(model.id)
                      return next
                    })
                  }
                />
                <span className="break-all">{model.name}</span>
              </label>
            ))}
          </div>
        )}
      </DialogBody>
      <DialogFooter>
        <Button disabled={saved.isPending || models.length === 0} onClick={() => saved.mutate()}>
          {t("actions.save")}
        </Button>
      </DialogFooter>
    </>
  )
}
