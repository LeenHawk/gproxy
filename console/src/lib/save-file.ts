/** What the Android shell exposes as `window.GproxyFiles` (see `GproxyFiles.kt`). */
type AndroidFiles = { save(name: string, mime: string, text: string): void }

/**
 * Hand a text file to the user.
 *
 * In a browser and the desktop window this is the usual `<a download>`. The
 * Android WebView silently drops that click — it has no download handler and
 * could not fetch a `blob:` URL anyway — so there the shell's own bridge opens
 * the system save dialog instead.
 */
export function saveTextFile(name: string, mime: string, text: string) {
  const android = (window as { GproxyFiles?: AndroidFiles }).GproxyFiles
  if (android) {
    android.save(name, mime, text)
    return
  }
  const url = URL.createObjectURL(new Blob([text], { type: mime }))
  const link = document.createElement("a")
  link.href = url
  link.download = name
  link.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}
