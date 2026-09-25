package dev.gproxy.desktop

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.provider.Settings
import android.util.Log
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.content.ContextCompat

/**
 * The window.
 *
 * Almost all of this is Tauri's: [TauriActivity] creates the WebView, wires
 * the IPC bridge and starts the Rust side. What is added is the order of four
 * things at startup, and the order is the whole of it.
 *
 * 1. **[GproxyNative.configure] first, before `super.onCreate()`.** The Rust
 *    side refuses to guess a data directory, and `super.onCreate()` is what
 *    eventually reaches `engine::ensure_started`. Configuring after it would
 *    be a race this would lose about half the time.
 * 2. **`super.onCreate()`**, which registers `WryLifecycleObserver` against
 *    `ProcessLifecycleOwner` and — the first time a window is opened in this
 *    process, and only the first time — calls `Rust.create()`, which runs
 *    `gproxy_host_tauri::start`. That anchoring to the *process* lifecycle
 *    rather than the activity's is what makes this whole arrangement legal:
 *    the foreground service can keep the process alive after this activity is
 *    destroyed, and a later activity re-attaches to the running engine
 *    instead of starting a second one. See `generated/WryActivity.kt`.
 * 3. **The service**, so the gateway survives the user leaving.
 * 4. **The two permissions** that are requests rather than manifest
 *    declarations.
 *
 * Nothing here talks to the engine. The window reaches it over Tauri's IPC
 * bridge, as the desktop window does, through the same 261 commands.
 */
class MainActivity : TauriActivity() {
    private val files = GproxyFiles(this)

    /** Before the console loads, so `window.GproxyFiles` exists on its first page. */
    override fun onWebViewCreate(webView: WebView) {
        webView.addJavascriptInterface(files, GproxyFiles.JS_NAME)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        GproxyNative.configure(this)
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        askToPostNotifications()
        askToIgnoreBatteryOptimisation()
        GproxyService.start(this)
    }

    /**
     * Without this the foreground service still runs, but its notification is
     * hidden — so the user gets no Stop button and no sign that anything is
     * happening. Asked once; a refusal is the user's to make and is not
     * asked again.
     */
    private fun askToPostNotifications() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            return
        }
        val granted =
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) ==
                PackageManager.PERMISSION_GRANTED
        if (!granted) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), REQUEST_NOTIFY)
        }
    }

    /**
     * Several vendors' power managers stop a foreground service anyway unless
     * the app is exempt, which for a gateway means going quietly off the air
     * overnight.
     *
     * Asked at most once ever, and recorded in the same preferences the boot
     * receiver reads: a dialog that reappeared on every launch would be the
     * kind of thing a person uninstalls an app over, and the answer does not
     * change by being asked again.
     */
    private fun askToIgnoreBatteryOptimisation() {
        val preferences = getSharedPreferences(GproxyService.PREFS, MODE_PRIVATE)
        if (preferences.getBoolean(PREF_ASKED_BATTERY, false)) {
            return
        }
        preferences.edit().putBoolean(PREF_ASKED_BATTERY, true).apply()
        val power = getSystemService(PowerManager::class.java)
        if (power?.isIgnoringBatteryOptimizations(packageName) == true) {
            return
        }
        try {
            startActivity(
                Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS)
                    .setData(Uri.parse("package:$packageName"))
            )
        } catch (error: Exception) {
            // Some builds ship no activity for this intent at all. It is an
            // optimisation, not a requirement, so it is a log line.
            Log.i(TAG, "this device offers no battery-optimisation exemption dialog", error)
        }
    }

    private companion object {
        const val TAG = "gproxy"
        const val REQUEST_NOTIFY = 10
        const val PREF_ASKED_BATTERY = "asked_battery_exemption"
    }
}
