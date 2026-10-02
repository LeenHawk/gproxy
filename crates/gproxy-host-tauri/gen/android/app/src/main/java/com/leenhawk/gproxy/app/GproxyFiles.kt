package com.leenhawk.gproxy.app

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.util.Log
import android.webkit.JavascriptInterface
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts

/**
 * Saving a file the console produced, somewhere the user can reach.
 *
 * The console saves an export the browser way — a `blob:` URL clicked through
 * an `<a download>` — and on Android that goes nowhere: the WebView Tauri
 * creates has no `DownloadListener`, and `DownloadManager` could not fetch a
 * `blob:` URL even if it had one. Everything the instance owns lives in
 * private storage (see [GproxyNative.dataDir]), so without this there is no
 * way to take a configuration off the phone at all.
 *
 * Exposed to the page as `window.GproxyFiles`. The console checks for it and
 * falls back to the browser path when it is absent, so the desktop window and
 * a plain browser are untouched.
 *
 * The page can only *propose* a file: the system document picker decides
 * where it goes and the user can cancel, so nothing reaches shared storage
 * without a person choosing it.
 *
 * Constructed as a property of the activity, because an activity-result
 * launcher must be registered before the activity is started.
 */
class GproxyFiles(private val activity: ComponentActivity) {
    /** The text waiting for the picker to answer. One at a time; a newer save replaces it. */
    private var pending: String? = null

    private val create =
        activity.registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            val text = pending
            pending = null
            val uri = result.data?.data
            if (result.resultCode != Activity.RESULT_OK || uri == null || text == null) {
                return@registerForActivityResult
            }
            write(uri, text)
        }

    /**
     * Called from the page on the WebView's JavaBridge thread, which is not
     * the thread an activity can be driven from.
     */
    @JavascriptInterface
    fun save(name: String, mime: String, text: String) {
        activity.runOnUiThread {
            pending = text
            val intent = Intent(Intent.ACTION_CREATE_DOCUMENT)
                .addCategory(Intent.CATEGORY_OPENABLE)
                .setType(mime)
                .putExtra(Intent.EXTRA_TITLE, name)
            try {
                create.launch(intent)
            } catch (error: Exception) {
                pending = null
                Log.e(TAG, "this device offers no document picker", error)
                toast(R.string.gproxy_save_failed)
            }
        }
    }

    @JavascriptInterface
    fun setupFinished() {
        activity.runOnUiThread { (activity as? MainActivity)?.startConfiguredService(askPermissions = false) }
    }

    @JavascriptInterface
    fun permissionStatus(): String {
        val notifications = androidx.core.app.NotificationManagerCompat.from(activity).areNotificationsEnabled()
        val power = activity.getSystemService(android.content.Context.POWER_SERVICE) as? android.os.PowerManager
        return org.json.JSONObject().put("notifications", notifications)
            .put("background", power?.isIgnoringBatteryOptimizations(activity.packageName) == true).toString()
    }

    @JavascriptInterface
    fun requestNotifications() {
        activity.runOnUiThread { (activity as? MainActivity)?.askToPostNotifications(manual = true) }
    }

    @JavascriptInterface
    fun requestBackground() {
        activity.runOnUiThread { (activity as? MainActivity)?.askToIgnoreBatteryOptimisation(manual = true) }
    }

    /** Off the main thread: an export can be megabytes and the target a cloud provider. */
    private fun write(uri: Uri, text: String) {
        Thread {
            val saved = try {
                activity.contentResolver.openOutputStream(uri, "wt")?.use {
                    it.write(text.toByteArray(Charsets.UTF_8))
                } != null
            } catch (error: Exception) {
                Log.e(TAG, "could not write $uri", error)
                false
            }
            activity.runOnUiThread { toast(if (saved) R.string.gproxy_save_done else R.string.gproxy_save_failed) }
        }.start()
    }

    private fun toast(message: Int) {
        Toast.makeText(activity, message, Toast.LENGTH_SHORT).show()
    }

    companion object {
        /** The name the page sees, `window.GproxyFiles`. */
        const val JS_NAME = "GproxyFiles"
        private const val TAG = "gproxy"
    }
}
