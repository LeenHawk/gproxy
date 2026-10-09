package com.leenhawk.gproxy.desktop

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.Process
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat

/**
 * What keeps the gateway alive when the user leaves the app.
 *
 * Android freezes and then kills a process with nothing in the foreground,
 * usually within seconds of the user switching away. A gateway that dies when
 * you switch apps is not a gateway, so this service exists for one reason: to
 * hold an ongoing notification, which is the only thing that tells Android
 * this process is doing work on the user's behalf.
 *
 * ## It supervises nothing, unlike v3's
 *
 * v3's `GproxyService.java` copied a `gproxy.bin` out of the APK's assets, set
 * `LD_LIBRARY_PATH`, started it as a child process, polled
 * `http://127.0.0.1:8787/admin` until it answered and pumped the child's
 * stdout into a ring buffer. Two processes, and the service's job was to
 * supervise the other one.
 *
 * Here the engine is in *this* process — it is compiled into the same
 * `libgproxy_host_tauri.so` the window runs on — so there is no child to
 * supervise, no asset to unpack, no health poll to write, and no way for the
 * two to disagree about which database they opened.
 *
 * ## Stopping ends the process, and that is deliberate
 *
 * The instance is assembled once per process and Tauri has already handed it
 * to 261 IPC commands as managed state. There is no honest "stop" that leaves
 * those commands holding a shut-down instance, so Stop closes the socket
 * ([GproxyNative.shutdown]) and then ends the process. The next start is a
 * cold one, which is also the only restart whose behaviour matches a first
 * launch.
 *
 * ## The foreground service type
 *
 * `specialUse` from Android 14, where the type is mandatory, and `dataSync`
 * below it. Not `dataSync` throughout: Android 15 caps a `dataSync` service at
 * six hours in any twenty-four, and a gateway that stops answering after six
 * hours is the same bug as one that stops when you switch apps. `specialUse`
 * has no such cap. Both are declared in the manifest.
 */
class GproxyService : Service() {
    @Volatile private var starting = false

    @Volatile private var running = false

    override fun onCreate() {
        super.onCreate()
        notifications().createNotificationChannel(
            NotificationChannel(
                CHANNEL,
                getString(R.string.gproxy_channel_name),
                NotificationManager.IMPORTANCE_LOW,
            ).apply { description = getString(R.string.gproxy_channel_description) }
        )
        // Android gives a service started with `startForegroundService` about
        // five seconds to post its notification before it kills the app, and
        // assembling an instance takes longer than that on a cold database. So
        // the notification goes up first and says "starting", and the work
        // happens on a worker in `onStartCommand`.
        enterForeground(getString(R.string.gproxy_status_starting))
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopEverything()
            return START_NOT_STICKY
        }
        if (running || starting) {
            return START_STICKY
        }
        starting = true
        // Never on the main thread. `ensure_started` opens SQLite, runs the
        // schema sync and binds a socket, and a main thread inside that is an
        // ANR waiting for the first touch event.
        Thread(::assemble, "gproxy-start").start()
        // `START_STICKY` so that a low-memory kill is recovered from: Android
        // re-delivers a null intent, which is not `ACTION_STOP`, and this runs
        // again in the fresh process.
        return START_STICKY
    }

    /** The instance, or a notification saying why there isn't one. */
    private fun assemble() {
        GproxyNative.configure(this)
        val status = GproxyNative.start()
        starting = false
        if (status.running) {
            running = true
            updateNotification(
                getString(R.string.gproxy_status_running, status.baseUrl.orEmpty()),
                if (status.secretsAreSealed) null else getString(R.string.gproxy_status_unsealed),
            )
        } else {
            reportFailure(status.error ?: getString(R.string.gproxy_status_failed_unknown))
        }
    }

    /**
     * A failed start stops the service, because there is nothing left to keep
     * alive — but the reason outlives it.
     *
     * v3 wrote the error into a ring buffer and tore the notification down
     * with the service, which left a person who had just tapped the icon
     * looking at nothing at all. The ongoing notification does go, since a
     * foreground service that is not serving anything is a lie; a separate
     * dismissible one carries the reason and survives.
     */
    private fun reportFailure(reason: String) {
        Log.e(TAG, "the instance did not start: $reason")
        notifications().notify(
            FAILURE_NOTIFICATION,
            NotificationCompat.Builder(this, CHANNEL)
                .setContentTitle(getString(R.string.gproxy_status_failed))
                .setContentText(reason)
                .setStyle(NotificationCompat.BigTextStyle().bigText(reason))
                .setSmallIcon(android.R.drawable.stat_notify_error)
                .setContentIntent(openTheWindow())
                .setAutoCancel(true)
                .build()
        )
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun stopEverything() {
        Log.i(TAG, "stopping at the user's request")
        GproxyNative.shutdown()
        running = false
        starting = false
        ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        stopSelf()
        // See the class note. The socket is already closed and the sync is
        // already stopped; this is what makes the next start a cold one.
        Process.killProcess(Process.myPid())
    }

    /**
     * The user swiped the task away. The gateway keeps running, which is the
     * whole point of `android:stopWithTask="false"` in the manifest and what
     * v3 chose too — a person who swipes a launcher card away has dismissed a
     * window, not revoked a service they explicitly started.
     *
     * Stop is in the notification, one tap away, and it is the only thing that
     * means "stop".
     */
    override fun onTaskRemoved(rootIntent: Intent?) {
        Log.i(TAG, "the task was removed; the instance keeps serving")
        super.onTaskRemoved(rootIntent)
    }

    /** Nothing binds to this; the engine is reached through JNI, not Binder. */
    override fun onBind(intent: Intent?): IBinder? = null

    private fun enterForeground(text: String) {
        ServiceCompat.startForeground(this, NOTIFICATION, notification(text, null), foregroundType())
    }

    private fun updateNotification(text: String, extra: String?) {
        notifications().notify(NOTIFICATION, notification(text, extra))
    }

    private fun notification(text: String, extra: String?) =
        NotificationCompat.Builder(this, CHANNEL)
            .setContentTitle(getString(R.string.app_name))
            .setContentText(text)
            .setStyle(
                NotificationCompat.BigTextStyle()
                    .bigText(if (extra == null) text else "$text\n$extra")
            )
            .setSmallIcon(android.R.drawable.stat_sys_download_done)
            .setContentIntent(openTheWindow())
            .apply {
                if (BuildConfig.SELF_UPDATE) {
                    addAction(
                        android.R.drawable.stat_sys_download,
                        getString(R.string.gproxy_action_update),
                        PendingIntent.getActivity(
                            this@GproxyService, 2,
                            Intent(this@GproxyService, GproxyUpdateActivity::class.java),
                            PendingIntent.FLAG_IMMUTABLE,
                        ),
                    )
                } else {
                    addAction(
                        android.R.drawable.ic_menu_info_details,
                        if (resources.configuration.locales[0].language == "zh") "隐私说明" else "Privacy",
                        PendingIntent.getActivity(
                            this@GproxyService, 2,
                            Intent(this@GproxyService, GproxyPrivacyActivity::class.java),
                            PendingIntent.FLAG_IMMUTABLE,
                        ),
                    )
                }
            }
            .addAction(
                android.R.drawable.ic_menu_close_clear_cancel,
                getString(R.string.gproxy_action_stop),
                PendingIntent.getService(
                    this,
                    1,
                    Intent(this, GproxyService::class.java).setAction(ACTION_STOP),
                    PendingIntent.FLAG_IMMUTABLE,
                ),
            )
            .setOngoing(true)
            .setSilent(true)
            .build()

    private fun openTheWindow() =
        PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP),
            PendingIntent.FLAG_IMMUTABLE,
        )

    private fun notifications() = getSystemService(NotificationManager::class.java)

    companion object {
        private const val TAG = "gproxy"

        /** The shared preferences the boot receiver reads. */
        const val PREFS = "gproxy"

        /** Whether the boot receiver starts the instance. On by default. */
        const val PREF_AUTO_START = "auto_start"

        const val ACTION_STOP = "com.leenhawk.gproxy.desktop.STOP"

        private const val CHANNEL = "gproxy-gateway"
        private const val NOTIFICATION = 8787
        private const val FAILURE_NOTIFICATION = 8789

        /**
         * Start the service, unless something already refused to let us.
         *
         * From Android 12 an app in the background cannot start a foreground
         * service; the two callers here are exempt — an activity in its
         * `onCreate` is in the foreground, and `BOOT_COMPLETED` is on the
         * platform's exemption list — but a caught refusal is a log line
         * rather than a crash for whatever the platform decides next.
         */
        fun start(context: Context) {
            try {
                context.startForegroundService(
                    Intent(context, GproxyService::class.java)
                )
            } catch (error: Exception) {
                Log.e(TAG, "the foreground service was refused", error)
            }
        }

        private fun setupChoices(context: Context): org.json.JSONObject? {
            val file = java.io.File(GproxyNative.dataDir(context), "desktop-setup.json")
            if (!file.isFile) return null
            return try { org.json.JSONObject(file.readText()) }
            catch (error: Exception) {
                Log.e(TAG, "could not read first-run settings", error)
                org.json.JSONObject().put("completed", false)
            }
        }

        fun setupComplete(context: Context): Boolean =
            setupChoices(context)?.optBoolean("completed", false)
                ?: java.io.File(GproxyNative.dataDir(context), "gproxy.db").isFile

        fun autoStarts(context: Context): Boolean {
            val choices = setupChoices(context)
            if (choices != null) return choices.optBoolean("completed", false) && choices.optBoolean("autoStart", false)
            return setupComplete(context) && context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
                .getBoolean(PREF_AUTO_START, true)
        }

        /**
         * `specialUse` where the platform demands a type, `dataSync` where it
         * understands one but not that one, nothing where it has no concept.
         * See the class note for why it is not `dataSync` all the way up.
         */
        private fun foregroundType(): Int =
            when {
                Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE ->
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
                // Store builds declare only the gateway's actual specialUse
                // type. Before Android 14, they can run an untyped service.
                BuildConfig.SELF_UPDATE && Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q ->
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
                else -> 0
            }
    }
}
