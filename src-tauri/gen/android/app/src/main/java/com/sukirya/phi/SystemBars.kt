package com.sukirya.phi

import android.app.Activity
import android.graphics.Color
import androidx.core.view.WindowCompat

/**
 * Pins the system bars to the app's light-only palette.
 *
 * The v1 UI has no dark theme, so edge-to-edge would otherwise leave status and
 * navigation icons white on the light background in night mode.
 */
fun Activity.applyLightSystemBars() {
  window.statusBarColor = Color.TRANSPARENT
  window.navigationBarColor = Color.TRANSPARENT
  WindowCompat.getInsetsController(window, window.decorView).apply {
    isAppearanceLightStatusBars = true
    isAppearanceLightNavigationBars = true
  }
}
