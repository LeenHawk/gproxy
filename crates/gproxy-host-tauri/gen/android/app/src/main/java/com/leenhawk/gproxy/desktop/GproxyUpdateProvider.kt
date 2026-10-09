package com.leenhawk.gproxy.desktop

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import java.io.File
import java.io.FileNotFoundException

/**
 * How the package installer gets to read an APK that lives in this app's
 * private storage.
 *
 * A downloaded update sits under `filesDir`, which nothing else on the device
 * can open — including the installer. A `content://` URI with
 * `FLAG_GRANT_READ_URI_PERMISSION` is the only way to hand one process a file
 * belonging to another, and since Android 7 a `file://` URI aimed at the
 * installer is a `FileUriExposedException` rather than an install.
 *
 * Written out rather than configured as an `androidx.core.content.FileProvider`
 * with an `<xml/paths>` file, exactly as v3 wrote it, because the surface is
 * smaller: this provider exposes **one** path, under one name, read-only, and
 * says so in code. A paths file that granted a directory would grant whatever
 * later ends up in that directory.
 *
 * Tauri's own generated `FileProvider` entry stays where it is under
 * `${applicationId}.fileprovider`; this is a second, narrower authority and
 * the two do not overlap.
 */
class GproxyUpdateProvider : ContentProvider() {
    override fun onCreate(): Boolean = true

    override fun getType(uri: Uri): String {
        requireApkUri(uri)
        return APK_TYPE
    }

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        requireApkUri(uri)
        if (mode != "r") {
            throw FileNotFoundException("read-only")
        }
        val context = context ?: throw FileNotFoundException("no context")
        return ParcelFileDescriptor.open(apkFile(context), ParcelFileDescriptor.MODE_READ_ONLY)
    }

    /**
     * The installer reads the name and the size before it will show a
     * confirmation, so those two columns are the whole implementation.
     */
    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor {
        requireApkUri(uri)
        val columns = projection ?: arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
        val cursor = MatrixCursor(columns, 1)
        val row = cursor.newRow()
        val length = context?.let { apkFile(it).length() } ?: 0L
        for (column in columns) {
            when (column) {
                OpenableColumns.DISPLAY_NAME -> row.add(APK_NAME)
                OpenableColumns.SIZE -> row.add(length)
                else -> row.add(null)
            }
        }
        return cursor
    }

    override fun insert(uri: Uri, values: ContentValues?): Uri =
        throw UnsupportedOperationException("read-only")

    override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = throw UnsupportedOperationException("read-only")

    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int =
        throw UnsupportedOperationException("read-only")

    companion object {
        /**
         * Taken from `BuildConfig` rather than spelled out, because the same
         * string is `${applicationId}.updates` in the manifest and the two
         * being one edit apart is how a provider ends up unreachable.
         */
        private val AUTHORITY = "${BuildConfig.APPLICATION_ID}.updates"

        private const val APK_NAME = "gproxy-update.apk"
        private const val MARKER_NAME = "install-apk.pending"
        private const val APK_TYPE = "application/vnd.android.package-archive"

        fun updateDir(context: Context): File = File(GproxyNative.dataDir(context), ".update")

        fun apkFile(context: Context): File = File(updateDir(context), APK_NAME)

        /**
         * The marker the downloader writes *after* it has verified the APK.
         *
         * Two files rather than one, exactly as v3 had it: a half-written
         * download is a file that exists, and an installer pointed at one is a
         * confusing failure at best. The marker is only created once the bytes
         * are complete and checked, so "is there an update" is one question
         * with one answer.
         */
        fun markerFile(context: Context): File = File(updateDir(context), MARKER_NAME)

        fun hasPendingUpdate(context: Context): Boolean =
            apkFile(context).isFile && markerFile(context).isFile

        fun apkUri(): Uri = Uri.parse("content://$AUTHORITY/$APK_NAME")

        private fun requireApkUri(uri: Uri) {
            require(uri.authority == AUTHORITY && uri.path == "/$APK_NAME") {
                "unsupported update URI"
            }
        }
    }
}
