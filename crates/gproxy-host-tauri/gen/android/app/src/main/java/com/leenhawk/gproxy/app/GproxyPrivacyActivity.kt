package com.leenhawk.gproxy.app

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.view.ViewGroup
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

/** Store launcher. Reading the notice never loads Tauri, the gateway, or a WebView. */
class GproxyPrivacyActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val viewOnly = intent.getBooleanExtra("viewOnly", false)
        if (!viewOnly && accepted(this)) {
            openApplication()
            return
        }
        val chinese = resources.configuration.locales[0].language == "zh"
        val policy = assets.open(if (chinese) "privacy-zh-CN.txt" else "privacy-en-US.txt")
            .bufferedReader().use { it.readText() }
        val padding = (24 * resources.displayMetrics.density).toInt()
        val content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(padding, padding, padding, padding)
        }
        val text = TextView(this).apply {
            this.text = policy
            textSize = 16f
            setTextIsSelectable(true)
        }
        content.addView(ScrollView(this).apply { addView(text) },
            LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
        content.addView(Button(this).apply {
            this.text = if (viewOnly) getString(android.R.string.ok) else if (chinese) "同意并继续" else "Agree and continue"
            setOnClickListener {
                if (viewOnly) finish() else {
                    getSharedPreferences(PREFERENCES, MODE_PRIVATE).edit()
                        .putString("acceptedVersion", BuildConfig.PRIVACY_VERSION).apply()
                    openApplication()
                }
            }
        })
        if (!viewOnly) content.addView(Button(this).apply {
            this.text = if (chinese) "不同意并退出" else "Decline and exit"
            setOnClickListener { finishAndRemoveTask() }
        })
        setContentView(content)
    }

    private fun openApplication() {
        startActivity(Intent(this, MainActivity::class.java))
        finish()
    }

    companion object {
        private const val PREFERENCES = "gproxy_privacy"

        fun accepted(context: Context): Boolean = BuildConfig.SELF_UPDATE ||
            context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
                .getString("acceptedVersion", null) == BuildConfig.PRIVACY_VERSION
    }
}
