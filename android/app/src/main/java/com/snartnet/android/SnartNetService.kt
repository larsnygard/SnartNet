package com.snartnet.android

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder

/**
 * Keeps the shared backend service alive while the app is not on screen (M10.2).
 *
 * The Rust service runs in this process, so without a foreground service Android is free to
 * kill the process as soon as the activity is gone — and with it a half-written mailbox pointer
 * or a queued message. The service is deliberately silent: it holds no wakelock and starts no
 * work of its own, so the sync mode the lifecycle reported decides what actually happens. It
 * exists to keep the process, not to keep the radio busy.
 */
class SnartNetService : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(NOTIFICATION_ID, notification())
        // If the process is killed anyway, the state on disk is what recovers it: the daemon
        // imports its store on the next start, outbox and inbound spool included (M10.3).
        return START_STICKY
    }

    private fun notification(): Notification {
        val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            manager.createNotificationChannel(
                NotificationChannel(
                    CHANNEL_ID,
                    "SnartNet",
                    NotificationManager.IMPORTANCE_LOW
                ).apply { description = "Keeps SnartNet reachable in the background" }
            )
        }
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }
        return builder
            .setContentTitle("SnartNet is running")
            .setContentText("Syncing while it can, paused when the battery is low.")
            .setSmallIcon(android.R.drawable.stat_sys_upload_done)
            .setOngoing(true)
            .build()
    }

    companion object {
        private const val CHANNEL_ID = "snartnet.background"
        private const val NOTIFICATION_ID = 1

        fun start(context: Context) {
            val intent = Intent(context, SnartNetService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, SnartNetService::class.java))
        }
    }
}
