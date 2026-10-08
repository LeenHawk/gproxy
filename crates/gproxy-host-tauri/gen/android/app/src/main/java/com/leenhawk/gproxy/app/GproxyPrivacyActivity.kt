package com.leenhawk.gproxy.app

import android.app.Activity
import android.os.Bundle
import android.view.ViewGroup
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView

/** Read-only offline privacy information, available from the service notification. */
class GproxyPrivacyActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
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
            this.text = getString(android.R.string.ok)
            setOnClickListener { finish() }
        })
        setContentView(content)
    }
}
