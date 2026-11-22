package com.sukirya.phi

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.material3.MaterialTheme
import com.chloemlla.lumen.crash.LumenCrash
import com.chloemlla.lumen.crash.ui.LumenCrashReportScreen

/**
 * Full-screen view of the report captured for the previous run.
 *
 * The screen belongs to the crash SDK: it renders the failure kind, the exit
 * reason (`REASON_CRASH_NATIVE`, ANR, …), the affected component and the
 * attribution footer, and offers copy/share/clear. The app only hosts it and
 * decides where "continue" goes.
 */
class CrashReportActivity : ComponentActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    val report = LumenCrash.loadPendingReportSafely()
    if (report == null) {
      launchApp()
      return
    }
    enableEdgeToEdge()
    applyLightSystemBars()
    setContent {
      MaterialTheme {
        // `onContinue` runs after the SDK cleared the stored report, so the
        // next launch lands on the app instead of this screen.
        LumenCrashReportScreen(report = report, onContinue = ::launchApp)
      }
    }
  }

  private fun launchApp() {
    startActivity(Intent(this, MainActivity::class.java))
    finish()
  }
}
