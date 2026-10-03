//! The publisher's notices, as the host filtered them for this build.
//!
//! The body is the Markdown subset the feed's checker admits — headings,
//! blockquotes, bullets, bold and code — rendered here by hand rather than
//! through a Markdown library: the subset is five rules, the feed is signed by
//! the project, and nothing else in the console needs a parser.

import { Fragment, type ReactNode } from "react"
import { useTranslation } from "react-i18next"
import { InfoIcon, OctagonAlertIcon, TriangleAlertIcon } from "lucide-react"
import type { Announcement, AnnouncementSeverity } from "@/api/update"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { formatInstant } from "@/lib/format"
import { cn } from "@/lib/utils"

const ICONS: Record<AnnouncementSeverity, typeof InfoIcon> = { info: InfoIcon, warning: TriangleAlertIcon, critical: OctagonAlertIcon }
const TONES: Record<AnnouncementSeverity, string> = {
  info: "border-state-info/40 [&>svg]:text-state-info",
  warning: "border-state-warning/40 [&>svg]:text-state-warning",
  critical: "border-destructive/50 [&>svg]:text-destructive",
}
const BADGES: Record<AnnouncementSeverity, "info" | "warning" | "destructive"> = { info: "info", warning: "warning", critical: "destructive" }

/** The notice in the console's language, else the English every entry carries. */
function localized(notice: Announcement, language: string) {
  return notice.content[language] ?? notice.content[language.split("-")[0]] ?? notice.content.en
}

export function AnnouncementList({ notices }: { notices: Array<Announcement> }) {
  const { t, i18n } = useTranslation()
  if (notices.length === 0) return <p className="text-sm text-muted-foreground">{t("update.announcements.empty")}</p>
  return (
    <ul className="flex flex-col gap-3">
      {notices.map((notice) => {
        const content = localized(notice, i18n.language)
        const Icon = ICONS[notice.severity] ?? InfoIcon
        const published = Date.parse(notice.published_at)
        return (
          <li key={notice.id}>
            <Alert className={cn("px-4 py-3", TONES[notice.severity] ?? TONES.info)}>
              <Icon aria-hidden />
              <AlertTitle className="flex flex-wrap items-center gap-2">
                <span>{content.title}</span>
                <Badge variant={BADGES[notice.severity] ?? "info"}>{t(`update.announcements.severity.${notice.severity}`)}</Badge>
                {Number.isFinite(published) ? <time dateTime={notice.published_at} className="text-xs font-normal text-muted-foreground">{formatInstant(published, i18n.language)}</time> : null}
              </AlertTitle>
              <AlertDescription className="block text-foreground/90"><AnnouncementBody text={content.body} /></AlertDescription>
            </Alert>
          </li>
        )
      })}
    </ul>
  )
}

type Block = { kind: "heading"; level: number; text: string } | { kind: "quote"; lines: Array<string> } | { kind: "list"; items: Array<string> } | { kind: "paragraph"; lines: Array<string> }

function blocks(text: string): Array<Block> {
  const out: Array<Block> = []
  for (const raw of text.replace(/\r\n?/g, "\n").split("\n")) {
    const line = raw.trimEnd()
    const last = out[out.length - 1]
    if (line.trim() === "") { if (last?.kind === "paragraph" || last?.kind === "quote" || last?.kind === "list") out.push({ kind: "paragraph", lines: [] }); continue }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line)
    if (heading) { out.push({ kind: "heading", level: heading[1].length, text: heading[2] }); continue }
    const quote = /^>\s?(.*)$/.exec(line)
    if (quote) { if (last?.kind === "quote") last.lines.push(quote[1]); else out.push({ kind: "quote", lines: [quote[1]] }); continue }
    const item = /^\s*[-*]\s+(.*)$/.exec(line)
    if (item) { if (last?.kind === "list") last.items.push(item[1]); else out.push({ kind: "list", items: [item[1]] }); continue }
    if (last?.kind === "paragraph") last.lines.push(line.trim()); else out.push({ kind: "paragraph", lines: [line.trim()] })
  }
  return out.filter((block) => block.kind !== "paragraph" || block.lines.length > 0)
}

/** `**bold**` and `` `code` ``; everything else is text. */
function inline(text: string): ReactNode {
  return text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g).map((part, index) => {
    if (part.startsWith("**") && part.endsWith("**") && part.length > 4) return <strong key={index}>{part.slice(2, -2)}</strong>
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2) return <code key={index} className="rounded bg-muted px-1 py-0.5 font-mono text-[0.85em]">{part.slice(1, -1)}</code>
    return <Fragment key={index}>{part}</Fragment>
  })
}

export function AnnouncementBody({ text }: { text: string }) {
  return (
    <div className="flex flex-col gap-2 text-sm leading-6">
      {blocks(text).map((block, index) => {
        switch (block.kind) {
          case "heading": return block.level <= 2 ? <h3 key={index} className="mt-1 text-sm font-semibold">{inline(block.text)}</h3> : <h4 key={index} className="mt-1 text-sm font-medium">{inline(block.text)}</h4>
          case "quote": return <blockquote key={index} className="border-l-2 border-border pl-3 text-muted-foreground">{block.lines.map((line, i) => <Fragment key={i}>{i > 0 ? " " : null}{inline(line)}</Fragment>)}</blockquote>
          case "list": return <ul key={index} className="list-disc pl-5">{block.items.map((item, i) => <li key={i}>{inline(item)}</li>)}</ul>
          case "paragraph": return <p key={index}>{block.lines.map((line, i) => <Fragment key={i}>{i > 0 ? " " : null}{inline(line)}</Fragment>)}</p>
        }
      })}
    </div>
  )
}
