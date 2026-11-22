package com.sukirya.phi

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import com.chloemlla.lumen.crash.LumenCrash

/**
 * Launcher entry point that routes a launch to the app or to the crash report.
 *
 * [MainActivity] extends `TauriActivity`, whose `onCreate` builds the Rust
 * webview — the very code path a crash may have come from. Showing the report
 * from there would re-enter the failing path and bury the report under a
 * webview that dies on start. This activity builds no webview, so the decision
 * is safe, and it disappears from the back stack as soon as it has routed.
 */
class LaunchGateActivity : Activity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    val next = if (pendingReportExists()) {
      CrashReportActivity::class.java
    } else {
      MainActivity::class.java
    }
    startActivity(Intent(this, next))
    finish()
  }

  private fun pendingReportExists(): Boolean =
    LumenCrash.isInstalled() && LumenCrash.loadPendingReportSafely() != null
}
