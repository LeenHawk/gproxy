package dev.gproxy.desktop

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log

/**
 * "Start at boot", which on Android is this and nothing else.
 *
 * A gateway that has to be opened by hand after every reboot is a gateway that
 * is down every time the phone restarts overnight and the user does not
 * notice until a client fails. So the same two intents v3 filtered are
 * filtered here:
 *
 * - `BOOT_COMPLETED`, the reboot, which needs the `RECEIVE_BOOT_COMPLETED`
 *   permission — without it the receiver is simply never called, silently;
 * - `MY_PACKAGE_REPLACED`, which is the *update*. An install that left the
 *   instance stopped until somebody happened to open the app would make every
 *   update an outage, and the in-app updater below exists precisely to make
 *   updates routine.
 *
 * It starts the service and returns. It deliberately does not touch the
 * engine: a broadcast receiver has about ten seconds of main thread before
 * Android declares it stuck, and assembling an instance can take longer than
 * that. [GproxyService] owns the worker thread that does the work.
 */
class GproxyBootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent?) {
        if (intent?.action !in STARTS) {
            return
        }
        if (!GproxyService.autoStarts(context)) {
            Log.i(TAG, "not starting at boot: auto-start is off")
            return
        }
        Log.i(TAG, "starting after ${intent?.action}")
        GproxyService.start(context)
    }

    private companion object {
        const val TAG = "gproxy"

        val STARTS =
            setOf(Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED)
    }
}
