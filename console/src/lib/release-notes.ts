import { createElement, type ReactNode } from "react";

export type ReleaseNoteLine =
  | { kind: "heading"; level: 2 | 3 | 4; content: ReactNode }
  | { kind: "blockquote" | "bullet" | "paragraph"; content: ReactNode };

const SECTION_HEADING = /^###\s+(English|简体中文)\s*$/;
const STABLE_VERSION = /^v?(\d+)\.(\d+)\.(\d+)$/;

export function selectNotesSection(markdown: string, language: string): string {
  const lines = markdown.split(/\r?\n/);
  const wanted = language.toLowerCase().startsWith("zh") ? "简体中文" : "English";
  const start = lines.findIndex((line) => SECTION_HEADING.exec(line.trim())?.[1] === wanted);
  const hasLanguageSections = lines.some((line) => SECTION_HEADING.test(line.trim()));

  if (!hasLanguageSections || start < 0) return markdown.trim();
  const following = lines.slice(start + 1);
  const nextSection = following.findIndex((line) => SECTION_HEADING.test(line.trim()));
  return following.slice(0, nextSection < 0 ? undefined : nextSection).join("\n").trim();
}

export function sortReleaseNotesDescending<T extends { version: string }>(entries: readonly T[]): T[] {
  return [...entries].sort((left, right) => compareStableVersions(right.version, left.version));
}

function compareStableVersions(left: string, right: string): number {
  const leftParts = STABLE_VERSION.exec(left)?.slice(1).map(Number);
  const rightParts = STABLE_VERSION.exec(right)?.slice(1).map(Number);
  if (!leftParts || !rightParts) return left.localeCompare(right, "en", { numeric: true });

  for (let index = 0; index < leftParts.length; index += 1) {
    const difference = leftParts[index] - rightParts[index];
    if (difference !== 0) return difference;
  }
  return 0;
}

export function parseReleaseNotes(markdown: string): ReleaseNoteLine[] {
  return markdown.split(/\r?\n/).flatMap<ReleaseNoteLine>((raw) => {
    const line = raw.trim();
    if (!line) return [];

    const heading = /^(#{2,4})\s+(.+)$/.exec(line);
    if (heading) {
      return [{
        kind: "heading" as const,
        level: heading[1].length as 2 | 3 | 4,
        content: inlineNodes(heading[2]),
      }];
    }
    if (line.startsWith(">")) {
      return [{ kind: "blockquote" as const, content: inlineNodes(line.slice(1).trim()) }];
    }
    if (line.startsWith("- ")) {
      return [{ kind: "bullet" as const, content: inlineNodes(line.slice(2).trim()) }];
    }
    return [{ kind: "paragraph" as const, content: inlineNodes(line) }];
  });
}

function inlineNodes(text: string): ReactNode {
  const nodes: ReactNode[] = [];
  const pattern = /(\*\*[^*]+\*\*|`[^`]+`)/g;
  let cursor = 0;

  for (const match of text.matchAll(pattern)) {
    const index = match.index;
    if (index > cursor) nodes.push(text.slice(cursor, index));
    const token = match[0];
    const content = token.startsWith("**") ? token.slice(2, -2) : token.slice(1, -1);
    nodes.push(createElement(token.startsWith("**") ? "strong" : "code", { key: index }, content));
    cursor = index + token.length;
  }
  if (cursor < text.length) nodes.push(text.slice(cursor));
  return nodes;
}
