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

    fun start(context: Context) {
        if (initialized) return
        initialized = true
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
    fun observe(observer: () -> Unit) { observers.add(observer); visible = true; observer() }
    fun remove(observer: () -> Unit) { observers.remove(observer); visible = observers.isNotEmpty() }
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
