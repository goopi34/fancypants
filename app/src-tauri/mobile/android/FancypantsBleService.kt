package io.github.goopi34.fancypants

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.os.Build
import android.os.IBinder

/**
 * Foreground service that keeps the Fancypants Rust Tokio runtime and BLE
 * connections alive when the phone screen is locked in a pocket or belt pouch.
 */
class FancypantsBleService : Service() {
    companion object {
        private const val CHANNEL_ID = "fancypants_ble_channel"
        private const val NOTIFICATION_ID = 4201
    }

    override fun onCreate() {
        super.onCreate()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Fancypants BLE Session",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Keeps Fancypants rangefinder and toy BLE connections active"
            }
            val manager = getSystemService(NotificationManager::class.java)
            manager?.createNotificationChannel(channel)
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }

        val notification = builder
            .setContentTitle("Fancypants Active")
            .setContentText("BLE rangefinder & toy controller running in background")
            .setSmallIcon(android.R.drawable.stat_sys_data_bluetooth)
            .setOngoing(true)
            .build()

        startForeground(NOTIFICATION_ID, notification)
        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null
}
