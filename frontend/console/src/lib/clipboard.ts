export type ClipboardLike = {
  writeText: (value: string) => Promise<void>;
};

export async function copyText(
  value: string,
  clipboard: ClipboardLike | undefined = typeof navigator === "undefined" ? undefined : navigator.clipboard,
) {
  if (!clipboard?.writeText) {
    throw new Error("Clipboard API unavailable");
  }

  await clipboard.writeText(value);
}
