package io.github.goopi34.fancypants

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat

class MainActivity : TauriActivity() {
    companion object {
        private const val PERMISSION_REQUEST_CODE = 4201
        private var btleplugInitialized = false

        init {
            System.loadLibrary("fancypants_app_lib")
        }

        @JvmStatic
        external fun initBtleplug()
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)

        // Ensure the current thread has the app's classloader attached and
        // initialize jni-utils + btleplug once on the main thread.
        if (!btleplugInitialized) {
            Thread.currentThread().contextClassLoader = classLoader
            try {
                initBtleplug()
                btleplugInitialized = true
            } catch (e: Throwable) {
                e.printStackTrace()
            }
        }

        requestBleAndNotificationPermissionsIfNeeded()
    }

    private fun requestBleAndNotificationPermissionsIfNeeded() {
        val needed = mutableListOf<String>()

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            if (ContextCompat.checkSelfPermission(this, Manifest.permission.BLUETOOTH_SCAN)
                != PackageManager.PERMISSION_GRANTED
            ) {
                needed.add(Manifest.permission.BLUETOOTH_SCAN)
            }
            if (ContextCompat.checkSelfPermission(this, Manifest.permission.BLUETOOTH_CONNECT)
                != PackageManager.PERMISSION_GRANTED
            ) {
                needed.add(Manifest.permission.BLUETOOTH_CONNECT)
            }
        } else {
            if (ContextCompat.checkSelfPermission(this, Manifest.permission.ACCESS_FINE_LOCATION)
                != PackageManager.PERMISSION_GRANTED
            ) {
                needed.add(Manifest.permission.ACCESS_FINE_LOCATION)
            }
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            if (ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS)
                != PackageManager.PERMISSION_GRANTED
            ) {
                needed.add(Manifest.permission.POST_NOTIFICATIONS)
            }
        }

        if (needed.isNotEmpty()) {
            ActivityCompat.requestPermissions(
                this,
                needed.toTypedArray(),
                PERMISSION_REQUEST_CODE
            )
        } else {
            startBleForegroundService()
        }
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == PERMISSION_REQUEST_CODE) {
            // Re-try btleplug init in case Adapter creation needed BLUETOOTH_CONNECT
            if (!btleplugInitialized) {
                try {
                    initBtleplug()
                    btleplugInitialized = true
                } catch (e: Throwable) {
                    e.printStackTrace()
                }
            }
            startBleForegroundService()
        }
    }

    private fun startBleForegroundService() {
        try {
            val serviceIntent = Intent(this, FancypantsBleService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                startForegroundService(serviceIntent)
            } else {
                startService(serviceIntent)
            }
        } catch (e: Throwable) {
            e.printStackTrace()
        }
    }
}
