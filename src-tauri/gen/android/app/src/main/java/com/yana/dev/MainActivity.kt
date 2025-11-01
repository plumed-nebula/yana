package com.yana.dev

import android.os.Bundle
import android.os.Build
import android.view.WindowManager
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import android.content.pm.PackageManager
import android.util.Log

class MainActivity : TauriActivity() {
  companion object {
    private const val PERMISSION_REQUEST_CODE = 1001
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    
    // 隐藏状态栏
    window.setFlags(
      WindowManager.LayoutParams.FLAG_FULLSCREEN,
      WindowManager.LayoutParams.FLAG_FULLSCREEN
    )
    
    super.onCreate(savedInstanceState)

    // Proactively request runtime permissions needed for file/network access on Android
    try {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
        val needed = mutableListOf<String>()
        val permissions = arrayOf(
          android.Manifest.permission.READ_EXTERNAL_STORAGE,
          android.Manifest.permission.WRITE_EXTERNAL_STORAGE,
          android.Manifest.permission.ACCESS_NETWORK_STATE,
          android.Manifest.permission.INTERNET,
          // Android 13+ media permission
          android.Manifest.permission.READ_MEDIA_IMAGES
        )

        for (p in permissions) {
          if (ContextCompat.checkSelfPermission(this, p) != PackageManager.PERMISSION_GRANTED) {
            // Only request permissions that are declared in the manifest
            if (PermissionHelper.hasDefinedPermission(this, p)) {
              needed.add(p)
            }
          }
        }

        if (needed.isNotEmpty()) {
          ActivityCompat.requestPermissions(this, needed.toTypedArray(), PERMISSION_REQUEST_CODE)
        }
      }
    } catch (e: Exception) {
      Log.w("MainActivity", "Failed to request runtime permissions: ${e}")
    }
  }

  override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<String>, grantResults: IntArray) {
    super.onRequestPermissionsResult(requestCode, permissions, grantResults)
    if (requestCode == PERMISSION_REQUEST_CODE) {
      for (i in permissions.indices) {
        val p = permissions[i]
        val granted = grantResults.getOrNull(i) == PackageManager.PERMISSION_GRANTED
        Log.d("MainActivity", "Permission result: $p -> $granted")
      }
    }
  }
}
