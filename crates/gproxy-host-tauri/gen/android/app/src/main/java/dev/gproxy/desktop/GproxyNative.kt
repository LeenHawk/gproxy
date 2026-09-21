package dev.gproxy.desktop

import android.content.Context
import android.util.Log
import java.io.File
import org.json.JSONObject

/**
 * The engine, from Kotlin.
 *
 * There are two ways into this process and only one of them is Tauri's. The
 * activity gets the IPC bridge; the foreground service and the boot receiver
 * get this — four `external` functions into the same `libgproxy_host_tauri.so`
 * the activity loads, reaching the same once-per-process instance. A boot
 * receiver has no window, so it could not use the bridge even if it wanted to.
 *
 * Everything here is thin on purpose. No decision is made in Kotlin that Rust
 * could make instead: [configure] names a directory, [start] blocks until
 * there is an instance or an error, and [status] reports. The 261-command
 * management surface stays where it is.
 */
object GproxyNative {
    private const val TAG = "gproxy"

    /**
     * The shared object Gradle builds from this crate. `WryActivity` loads the
     * same name from its own static initialiser; a second `loadLibrary` for a
     * library already in this process is a no-op, which is what lets the
     * service run without an activity ever existing.
     */
    private const val LIBRARY = "gproxy_host_tauri"

    /**
     * The instance's directory, under the app's private storage.
     *
     * A subdirectory rather than `filesDir` itself: `filesDir` is also where
     * an APK download and anything else app-private lands, and a database
     * directory that shares a root with everything else is a directory nobody
     * can clear safely.
     */
    fun dataDir(context: Context): File = File(context.filesDir, "gproxy")

    /**
     * Load the library and tell it where the instance lives.
     *
     * Must run before anything starts the engine — the activity calls it at
     * the top of `onCreate`, the service before it starts its worker — because
     * the Rust side deliberately refuses to guess a directory.
     */
    @Synchronized
    fun configure(context: Context) {
        if (!loaded) {
            System.loadLibrary(LIBRARY)
            loaded = true
        }
        val dir = dataDir(context)
        if (!dir.isDirectory && !dir.mkdirs()) {
            Log.e(TAG, "could not create the data directory at ${dir.absolutePath}")
        }
        nativeConfigure(dir.absolutePath)
    }

    /** Assemble the instance if this process has not, and say what happened. */
    fun start(): Status = parse(nativeStart())

    /** What this process has right now, assembling nothing. */
    fun status(): Status = parse(nativeStatus())

    /**
     * Close the data plane's socket and stop the background sync.
     *
     * The caller ends the process straight afterwards. See `engine.rs`: the
     * instance is assembled once per process and Tauri has already handed it
     * to every IPC command as managed state, so "stop" cannot mean "put it
     * back how it was".
     */
    fun shutdown() = nativeShutdown()

    /**
     * What the notification shows and what a failure says.
     *
     * A JSON string across JNI rather than a struct: a Kotlin data class built
     * field by field from Rust would be a second declaration of the same four
     * values, kept in step by hand, for no gain over one `String`.
     */
    data class Status(
        val running: Boolean,
        val baseUrl: String?,
        val secretsAreSealed: Boolean,
        val error: String?,
    )

    private var loaded = false

    /**
     * A null or unparseable answer is a bug in this file's pairing with
     * `android.rs`, not a condition a phone should crash on: it becomes a
     * "not running" with the reason in it, which is exactly what the
     * notification is for.
     */
    private fun parse(json: String?): Status {
        if (json == null) {
            return Status(false, null, false, "the native library answered nothing")
        }
        return try {
            val parsed = JSONObject(json)
            Status(
                running = parsed.optBoolean("running", false),
                baseUrl = parsed.optString("baseUrl", "").ifEmpty { null },
                secretsAreSealed = parsed.optBoolean("secretsAreSealed", false),
                error = parsed.optString("error", "").ifEmpty { null },
            )
        } catch (error: Exception) {
            Log.e(TAG, "unparseable status from the native library: $json", error)
            Status(false, null, false, error.toString())
        }
    }

    private external fun nativeConfigure(dataDir: String)

    private external fun nativeStart(): String?

    private external fun nativeStatus(): String?

    private external fun nativeShutdown()
}
