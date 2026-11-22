package com.sukirya.phi

import android.app.Application
import android.util.Log
import com.chloemlla.lumen.crash.LumenCrash
import java.io.File

/**
 * Process entry point for the Android shell.
 *
 * Crash reporting is installed here rather than in an activity so that a death
 * anywhere in the process — including the Rust runtime, which the JVM cannot
 * observe — is captured on the next launch.
 */
class PhiApplication : Application() {
  override fun onCreate() {
    super.onCreate()
    installCrashReporting()
  }

  private fun installCrashReporting() {
    // Shared with `src-tauri/src/android.rs`, which writes panic dumps here
    // while the process is dying. Creating it early means the Rust side never
    // has to deal with a missing directory.
    val inbox = File(filesDir, CRASH_INBOX_DIR_NAME)
    runCatching { inbox.mkdirs() }

    val installed = LumenCrash.installSafely(this) {
      appDisplayName = getString(R.string.app_name)
      externalInboxDirectory = inbox
      // phi-agent is local-first: reports stay on the device until the user
      // copies or shares one, so the SDK's default backend upload is off.
      crashReportBackendEnabled = false
      anrWatchdogEnabled = true
      priorExitCaptureEnabled = true
      // The webview owns first paint, so "the UI never became visible" cannot be
      // judged from the Android side; a slow cold start would be reported as a
      // hang. Freezes are still detected once the app is interactive.
      startupHangWatchdogEnabled = false
    }
    if (!installed) {
      Log.e(TAG, "crash reporting is unavailable: the SDK refused to install")
    }
  }

  private companion object {
    const val TAG = "PhiApplication"

    /** Also used by `src-tauri/src/android.rs`; keep both in sync. */
    const val CRASH_INBOX_DIR_NAME = "crash-inbox"
  }
}
