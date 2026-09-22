package dev.gproxy.desktop

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.util.Log
import android.widget.Toast
import android.widget.TextView
import android.view.Gravity

/**
 * Installing the APK that was downloaded.
 *
 * On a phone there is no package manager to update from and, for a sideloaded
 * gateway, no store either — so "update" means: fetch the next APK, hand it to
 * the system installer, and let the user confirm. The confirmation is not
 * optional and cannot be automated without a device owner; the most an app can
 * do is get the user to the right dialog with one tap, which is what this is.
 *
 * Two permissions stand between here and the dialog, and they are different
 * things:
 *
 * - `REQUEST_INSTALL_PACKAGES` in the manifest, which is what makes the intent
 *   legal at all;
 * - the per-app "install unknown apps" toggle in Settings, which is a user
 *   decision and can only be *asked for* with
 *   `ACTION_MANAGE_UNKNOWN_APP_SOURCES`. An install intent fired without it is
 *   silently refused, so it is checked first and the settings screen comes up
 *   instead.
 *
 * The native updater downloads and verifies on a worker thread. It publishes
 * the marker only after signature, compatibility, size and hash checks pass.
 */
class GproxyUpdateActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        GproxyNative.configure(this)
        if (GproxyUpdateProvider.hasPendingUpdate(this)) {
            continueInstall()
            return
        }
        setContentView(TextView(this).apply {
            setText(R.string.gproxy_update_checking)
            gravity = Gravity.CENTER
        })
        Thread({
            val result = runCatching { GproxyNative.update() }
            runOnUiThread {
                if (isFinishing || isDestroyed) return@runOnUiThread
                result.fold(
                    onSuccess = { response ->
                        when {
                            response.has("error") -> showFailure(response.getString("error"))
                            response.getBoolean("ready") -> continueInstall()
                            else -> {
                                Toast.makeText(this, R.string.gproxy_update_current, Toast.LENGTH_LONG).show()
                                finish()
                            }
                        }
                    },
                    onFailure = { showFailure(it.toString()) },
                )
            }
        }, "gproxy-update").start()
    }

    private fun showFailure(reason: String) {
        Log.e(TAG, "update failed: $reason")
        android.app.AlertDialog.Builder(this)
            .setTitle(R.string.gproxy_update_failed)
            .setMessage(reason)
            .setPositiveButton(android.R.string.ok) { _, _ -> finish() }
            .setOnCancelListener { finish() }
            .show()
    }

    @Deprecated("startActivityForResult is the API that exists on a bare Activity")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        @Suppress("DEPRECATION")
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == REQUEST_UNKNOWN_SOURCES && canInstallPackages()) {
            openPackageInstaller()
        } else {
            finish()
        }
    }

    private fun continueInstall() {
        when {
            !GproxyUpdateProvider.hasPendingUpdate(this) -> {
                Toast.makeText(this, R.string.gproxy_update_none, Toast.LENGTH_SHORT).show()
                finish()
            }
            canInstallPackages() -> openPackageInstaller()
            else -> {
                @Suppress("DEPRECATION")
                startActivityForResult(
                    Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES)
                        .setData(Uri.parse("package:$packageName")),
                    REQUEST_UNKNOWN_SOURCES,
                )
            }
        }
    }

    private fun canInstallPackages(): Boolean = packageManager.canRequestPackageInstalls()

    private fun openPackageInstaller() {
        Log.i(TAG, "handing the update to the package installer")
        @Suppress("DEPRECATION")
        startActivityForResult(
            Intent(Intent.ACTION_VIEW)
                .setDataAndType(
                    GproxyUpdateProvider.apkUri(),
                    "application/vnd.android.package-archive",
                )
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION),
            REQUEST_INSTALL,
        )
        // The marker goes as soon as the installer has the URI, not when the
        // install succeeds — which this app cannot observe anyway, since a
        // successful install replaces it. Leaving it would mean re-offering
        // the same APK on every launch after the user declined once.
        GproxyUpdateProvider.markerFile(this).delete()
    }

    companion object {
        private const val TAG = "gproxy"
        private const val REQUEST_UNKNOWN_SOURCES = 1
        private const val REQUEST_INSTALL = 2

        fun launch(context: Context) {
            context.startActivity(
                Intent(context, GproxyUpdateActivity::class.java)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
            )
        }
    }
}
