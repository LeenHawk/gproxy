import { useMemo } from "react";
import { parseReleaseNotes, type ReleaseNoteLine } from "@/lib/release-notes";
import { cn } from "@/lib/utils";

export function MarkdownContent({ markdown, className }: { markdown: string; className?: string }) {
  const lines = useMemo(() => parseReleaseNotes(markdown), [markdown]);
  return (
    <div className={cn("space-y-3", className)}>
      {lines.map((line, index) => <MarkdownRow key={index} line={line} />)}
    </div>
  );
}

function MarkdownRow({ line }: { line: ReleaseNoteLine }) {
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
