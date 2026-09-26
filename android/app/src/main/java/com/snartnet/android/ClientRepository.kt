package com.snartnet.android

import android.content.Context
import android.os.Handler
import android.os.Looper
import org.json.JSONObject
import java.io.File
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/** Process-owned state survives activity recreation; drafts are never written to disk. */
object ClientRepository {
    private val commands = Executors.newSingleThreadExecutor()
    private val network = Executors.newSingleThreadScheduledExecutor()
    private val main = Handler(Looper.getMainLooper())
    private val observers = mutableSetOf<() -> Unit>()
    var state = JSONObject()
        private set
    var status = "Loading local identity…"
        private set
    @Volatile var ready = false
        private set
    var syncing = false
        private set
    var screen = "Messages"
    var selected: String? = null
    val forms = mutableMapOf<String, String>()
    val drafts = mutableMapOf<String, String>()
    val ciphertext = mutableSetOf<String>()
    private val pending = java.util.concurrent.ConcurrentHashMap.newKeySet<String>()
    private var initialized = false
    @Volatile private var visible = false
    private var context: Context? = null
    private val powerManager: android.os.PowerManager?
        get() = context?.getSystemService(Context.POWER_SERVICE) as? android.os.PowerManager

    fun start(context: Context) {
        if (initialized) return
        initialized = true
        this.context = context.applicationContext
        registerPowerObservers(context.applicationContext)
        val root = File(context.filesDir, "client").absolutePath
        commands.execute {
            try {
                val initial = decode(NativeBridge.nativeInit(root))
                main.post {
                    state = initial; ready = true
                    status = "Ready"
                    if (state.isNull("profile")) screen = "My profile"
                    changed()
                }
            } catch (e: Throwable) { error(e) }
        }
        network.scheduleWithFixedDelay({ if (visible) sync() }, 2, 4, TimeUnit.SECONDS)
    }
    fun observe(observer: () -> Unit) { observers.add(observer); visible = true; reportLifecycle(); observer() }
    fun remove(observer: () -> Unit) {
        observers.remove(observer)
        visible = observers.isNotEmpty()
        reportLifecycle()
    }

    /**
     * Tell the backend service what the app and the device are doing (M10.2). The service owns
     * the sync cadence, so this is how a phone that is hidden and saving battery stops working
     * instead of waking the radio every minute.
     */
    private fun reportLifecycle() {
        val powerSave = powerManager?.isPowerSaveMode ?: false
        val charging = batteryCharging()
        commands.execute {
            try {
                decode(NativeBridge.nativeSetLifecycle(visible, powerSave, charging))
            } catch (e: Throwable) {
                // A lifecycle report is not worth a user-visible error: the next one retries.
            }
        }
    }

    /** A power-save or charging change invalidates what the service was last told. */
    fun lifecycleChanged() = reportLifecycle()

    /**
     * Whether the device is charging. Read from the sticky battery broadcast so it works back to
     * API 24, and defaulted to "not charging" when the platform says nothing.
     */
    private fun batteryCharging(): Boolean {
        val status = context
            ?.registerReceiver(null, android.content.IntentFilter(android.content.Intent.ACTION_BATTERY_CHANGED))
            ?.getIntExtra(android.os.BatteryManager.EXTRA_STATUS, -1)
            ?: -1
        return status == android.os.BatteryManager.BATTERY_STATUS_CHARGING ||
            status == android.os.BatteryManager.BATTERY_STATUS_FULL
    }

    private fun registerPowerObservers(source: Context) {
        val filter = android.content.IntentFilter().apply {
            addAction(android.os.PowerManager.ACTION_POWER_SAVE_MODE_CHANGED)
            addAction(android.content.Intent.ACTION_POWER_CONNECTED)
            addAction(android.content.Intent.ACTION_POWER_DISCONNECTED)
        }
        source.registerReceiver(object : android.content.BroadcastReceiver() {
            override fun onReceive(context: Context?, intent: android.content.Intent?) = lifecycleChanged()
        }, filter)
    }
    private fun changed() { observers.toList().forEach { it() } }
    private fun decode(raw: String): JSONObject {
        val result = JSONObject(raw)
        if (!result.optBoolean("ok")) throw IllegalStateException(result.optString("error", "Operation failed"))
        return result.getJSONObject("payload")
    }
    private fun error(e: Throwable) { main.post { status = e.message ?: "Operation failed"; changed() } }
    fun command(request: JSONObject, success: String? = null, finished: (() -> Unit)? = null, done: ((JSONObject) -> Unit)? = null) {
        if (!ready) return
        val operation = request.optString("op")
        val key = if (operation in listOf("message", "profile", "post")) operation + request.optString("recipient") else null
        if (key != null && !pending.add(key)) { finished?.invoke(); return }
        commands.execute {
            try {
                val result = decode(NativeBridge.nativeCommand(request.toString()))
                main.post {
                    if (result.has("threads")) state = result
                    if (success != null) status = success
                    if (key != null) pending.remove(key)
                    finished?.invoke()
                    done?.invoke(result)
                    changed()
                }
            } catch (e: Throwable) {
                main.post { if (key != null) pending.remove(key); finished?.invoke() }
                error(e)
            }
        }
    }
    fun syncNow() { network.execute { sync() } }
    private fun sync() {
        if (!ready) return
        main.post { syncing = true; changed() }
        try {
            decode(NativeBridge.nativeSync())
            command(JSONObject().put("op", "snapshot"))
        } catch (e: Throwable) { error(e) }
        finally { main.post { syncing = false; changed() } }
    }
}
