// Schema: {version:1, notifications:[{id, severity, published_at, expires_at?,
// affects?, content:{en:{title, body}, "zh-CN"?:{title, body}, "zh-TW"?:{title, body}}}]}.
// severity is info|warning|critical; dates are RFC3339; affects is a comma-separated
// semver comparator range; body supports headings, blockquotes, bullets, bold, and code.
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const docsDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const file = path.join(docsDir, "public", "notifications.json");
const errors = [];
let feed;

try {
  feed = JSON.parse(await readFile(file, "utf8"));
} catch (error) {
  console.error(`notifications.json is not valid JSON: ${error.message}`);
  process.exit(1);
}

const object = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value) => typeof value === "string" && value.trim().length > 0;
const rfc3339 = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(?:Z|[+-](\d{2}):(\d{2}))$/;
const validDate = (value) => {
  const match = text(value) && rfc3339.exec(value);
  if (!match) return false;
  const [year, month, day, hour, minute, second] = match.slice(1, 7).map(Number);
  const offsetHour = Number(match[7] ?? 0);
  const offsetMinute = Number(match[8] ?? 0);
  const calendar = new Date(Date.UTC(year, month - 1, day));
  return calendar.getUTCFullYear() === year && calendar.getUTCMonth() === month - 1
    && calendar.getUTCDate() === day && hour < 24 && minute < 60 && second < 60
    && offsetHour < 24 && offsetMinute < 60 && !Number.isNaN(Date.parse(value));
};
const comparator = /^(?:=|>=|>|<=|<|\^|~)?(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$/;
const validRange = (value) => text(value) && value.split(",").every((part) => comparator.test(part.trim()));

if (!object(feed) || feed.version !== 1 || !Array.isArray(feed.notifications)) {
  errors.push("root must contain version 1 and a notifications array");
} else {
  const ids = new Set();
  feed.notifications.forEach((entry, index) => {
    const at = `notifications[${index}]`;
    if (!object(entry)) return errors.push(`${at} must be an object`);
    if (!text(entry.id)) errors.push(`${at}.id must be a non-empty string`);
    else if (ids.has(entry.id)) errors.push(`${at}.id must be unique`);
    else ids.add(entry.id);
    if (!["info", "warning", "critical"].includes(entry.severity)) errors.push(`${at}.severity is invalid`);
    if (!validDate(entry.published_at)) errors.push(`${at}.published_at must be RFC3339`);
    if (entry.expires_at !== undefined && !validDate(entry.expires_at)) errors.push(`${at}.expires_at must be RFC3339`);
    if (entry.affects !== undefined && !validRange(entry.affects)) errors.push(`${at}.affects must be a semver range`);
    if (!object(entry.content) || !object(entry.content.en)) errors.push(`${at}.content.en is required`);
    for (const [locale, content] of Object.entries(entry.content ?? {})) {
      if (!["en", "zh-CN", "zh-TW"].includes(locale)) errors.push(`${at}.content has unsupported locale ${locale}`);
      if (!object(content) || !text(content.title) || !text(content.body)) errors.push(`${at}.content.${locale} needs title and body`);
    }
  });
}

if (errors.length > 0) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log(`notifications.json is valid (${feed.notifications.length} entries).`);
