package com.sukirya.phi

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // The v1 UI is light-only. Edge-to-edge leaves the bars transparent, so the
    // icon appearance must be pinned to light regardless of the system theme.
    applyLightSystemBars()
  }
}
