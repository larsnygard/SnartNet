package com.snartnet.android

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.Typeface
import android.net.Uri
import android.os.Bundle
import android.text.InputType
import android.util.Base64
import android.view.View
import android.widget.*
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.widget.doAfterTextChanged
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.concurrent.Executors

class MainActivity : AppCompatActivity() {
    private val repo = ClientRepository
    private lateinit var body: LinearLayout
    private lateinit var status: TextView
    private lateinit var title: TextView
    private var dynamic: LinearLayout? = null
    private var rendered = ""
    private var revision = ""
    private val observer: () -> Unit = { update() }
    private val io = Executors.newSingleThreadExecutor()
    private var exportFormat = "png"
    private var exporting = false
    private var pendingRead: String? = null
    private var sending = false
    private var inviteImage: ImageView? = null
    private val ink = Color.rgb(25, 43, 50)
    private val muted = Color.rgb(90, 111, 116)
    private val teal = Color.rgb(0, 103, 97)

    private val avatarPicker = registerForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri != null) loadAvatar(uri)
    }
    private val qrPicker = registerForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri != null) importQr(uri)
    }
    private val exporter = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        val uri = result.data?.data
        if (result.resultCode == RESULT_OK && uri != null) {
            repo.command(op("qr").put("format", exportFormat)) { payload ->
                val bytes = Base64.decode(payload.getString("data"), Base64.DEFAULT)
                io.execute {
                    try { contentResolver.openOutputStream(uri)?.use { it.write(bytes) } ?: error("Cannot open destination")
                        runOnUiThread { toast("Invitation saved") }
                    } catch (e: Exception) { runOnUiThread { toast(e.message ?: "Export failed") } }
                }
            }
        }
        exporting = false
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        exportFormat = savedInstanceState?.getString("exportFormat") ?: "png"
        exporting = savedInstanceState?.getBoolean("exporting") ?: false
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.rgb(246, 248, 246))
        }
        ViewCompat.setOnApplyWindowInsetsListener(root) { v, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.ime())
            v.setPadding(bars.left, bars.top, bars.right, bars.bottom); insets
        }
        title = label("SnartNet", 27).apply { setPadding(dp(20), dp(20), dp(20), dp(8)); setTypeface(null, Typeface.BOLD) }
        root.addView(title)
        val navScroll = HorizontalScrollView(this).apply { isHorizontalScrollBarEnabled = false }
        val nav = LinearLayout(this)
        for (screen in listOf("Messages", "Feed", "Contacts", "My profile", "Network")) {
            nav.addView(button(screen) { repo.screen = screen; rendered = ""; update() })
        }
        navScroll.addView(nav); root.addView(navScroll)
        status = label("Loading…", 12).apply { setPadding(dp(20), dp(8), dp(20), dp(8)); setTextColor(muted) }
        root.addView(status)
        val scroll = ScrollView(this).apply { isFillViewport = true }
        body = column().apply { setPadding(dp(20), dp(8), dp(20), dp(24)) }
        scroll.addView(body); root.addView(scroll, LinearLayout.LayoutParams(-1, 0, 1f))
        setContentView(root)
        if (savedInstanceState == null) acceptIntent(intent)
        repo.start(applicationContext)
    }
    override fun onStart() { super.onStart(); repo.observe(observer) }
    override fun onStop() { repo.remove(observer); super.onStop() }
    override fun onDestroy() { io.shutdown(); super.onDestroy() }
    override fun onSaveInstanceState(out: Bundle) {
        out.putString("exportFormat", exportFormat); out.putBoolean("exporting", exporting)
        super.onSaveInstanceState(out)
    }
    override fun onNewIntent(intent: Intent) { super.onNewIntent(intent); setIntent(intent); acceptIntent(intent); rendered = ""; update() }
    private fun acceptIntent(intent: Intent) {
        val link = intent.dataString ?: intent.getStringExtra(Intent.EXTRA_TEXT) ?: return
        if (link.startsWith("snartnet://invite/")) {
            repo.forms["contact.input"] = link; repo.screen = "Contacts"
        }
    }
    private fun op(name: String) = JSONObject().put("op", name)
    private fun dp(n: Int) = (resources.displayMetrics.density * n).toInt()
    private fun column() = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
    private fun label(value: String, size: Int = 16) = TextView(this).apply {
        text = value; textSize = size.toFloat(); setTextColor(ink)
        setPadding(0, dp(6), 0, dp(6))
    }
    private fun button(value: String, action: () -> Unit) = Button(this).apply {
        text = value; isAllCaps = false; setTextColor(teal); setOnClickListener { action() }
    }
    private fun heading(value: String) { body.addView(label(value, 22).apply { setTypeface(null, Typeface.BOLD) }) }
    private fun hint(value: String, parent: LinearLayout = body) { parent.addView(label(value, 14).apply { setTextColor(muted) }) }
    private fun input(key: String, caption: String, initial: String = "", multiline: Boolean = false): EditText {
        body.addView(label(caption, 13))
        return EditText(this).apply {
            hint = caption; setTextColor(ink); setTextSize(16f)
            inputType = InputType.TYPE_CLASS_TEXT or if (multiline) InputType.TYPE_TEXT_FLAG_MULTI_LINE or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES else InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
            setSingleLine(!multiline); if (multiline) minLines = 3
            setText(repo.forms.getOrPut(key) { initial })
            doAfterTextChanged { repo.forms[key] = it.toString() }
            body.addView(this, LinearLayout.LayoutParams(-1, -2))
        }
    }
    private fun array(name: String) = repo.state.optJSONArray(name) ?: JSONArray()
    private fun objects(array: JSONArray) = (0 until array.length()).map { array.getJSONObject(it) }
    private fun text(obj: JSONObject?, key: String, fallback: String = ""): String = if (obj == null || obj.isNull(key)) fallback else obj.optString(key, fallback)
    private fun toast(value: String) { Toast.makeText(this, value, Toast.LENGTH_LONG).show() }
    private fun copy(value: String) {
        getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("SnartNet invitation", value)); toast("Copied")
    }

    private fun update() {
        if (isFinishing || isDestroyed) return
        status.text = repo.status + if (repo.syncing) " · Syncing…" else ""
        val unread = objects(array("threads")).sumOf { it.optInt("unread") }
        title.text = if (unread > 0) "SnartNet · $unread unread" else "SnartNet"
        if (!repo.ready) return
        val key = repo.screen + ":" + repo.selected
        if (rendered != key) {
            rendered = key; revision = ""; body.removeAllViews(); dynamic = null; inviteImage = null
            heading(repo.screen)
            when (repo.screen) {
                "My profile" -> profile()
                "Contacts" -> contacts()
                "Messages" -> messages()
                "Feed" -> feed()
                "Network" -> network()
            }
        }
        val next = repo.state.toString()
        if (revision != next) { revision = next; refreshDynamic() }
    }

    private fun profile() {
        val profile = repo.state.optJSONObject("profile")
        hint("Your identity and profile stay on this device.")
        input("profile.username", "Username", text(profile, "username"))
        input("profile.display", "Display name", text(profile, "display_name"))
        input("profile.bio", "Bio", text(profile, "bio"), true)
        input("profile.address", "Connection address (optional IP:port)", text(repo.state, "address"))
        repo.forms.getOrPut("profile.avatar") { text(profile, "avatar_data_url") }
        val avatar = ImageView(this)
        setAvatar(avatar, repo.forms["profile.avatar"] ?: "")
        body.addView(avatar, LinearLayout.LayoutParams(dp(96), dp(96)))
        body.addView(button("Choose profile picture") { avatarPicker.launch("image/*") })
        body.addView(button("Remove picture") { repo.forms["profile.avatar"] = ""; rendered = ""; update() })
        val save = button("Save profile") { }
        save.setOnClickListener {
            save.isEnabled = false
            repo.command(op("profile").put("username", repo.forms["profile.username"])
                .put("displayName", repo.forms["profile.display"]).put("bio", repo.forms["profile.bio"])
                .put("address", repo.forms["profile.address"]).put("avatar", repo.forms["profile.avatar"]), "Profile saved", finished = { save.isEnabled = true }) {
                repo.syncNow(); rendered = ""; update()
            }
        }
        body.addView(save)
        if (profile != null) {
            hint("Fingerprint: ${text(profile, "fingerprint")}")
            body.addView(button("Copy invitation link") { repo.command(op("invite")) { copy(it.getString("uri")) } })
            body.addView(button("Share invitation") {
                repo.command(op("invite")) {
                    startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, it.getString("uri")), "Share invitation"))
                }
            })
            body.addView(button("Copy profile magnet") { repo.command(op("invite")) { copy(text(it, "magnet")) } })
            inviteImage = ImageView(this).also { body.addView(it, LinearLayout.LayoutParams(-1, dp(280))) }
            repo.command(op("qr")) { result ->
                val bytes = Base64.decode(result.getString("data"), Base64.DEFAULT)
                inviteImage?.setImageBitmap(BitmapFactory.decodeByteArray(bytes, 0, bytes.size))
            }
            for (format in listOf("png", "jpg", "svg")) body.addView(button("Save QR ${format.uppercase()}") { export(format) })
        }
    }

    private fun contacts() {
        hint("Exchange invitations in both directions, then sync to verify profiles and encryption keys.")
        input("contact.input", "Invitation, profile magnet, or fingerprint", multiline = true)
        input("contact.alias", "Alias (optional)")
        input("contact.address", "Peer IP:port (optional)")
        body.addView(button("Add / update contact") {
            val value = repo.forms["contact.input"].orEmpty().trim()
            val manual = !value.startsWith("snartnet:") && !value.startsWith("magnet:") && value.length == 24
            repo.command(op("contact").put("input", value).put("mode", if (manual) "manual" else "invite")
                .put("alias", repo.forms["contact.alias"]).put("address", repo.forms["contact.address"]), "Contact saved") { repo.syncNow() }
        })
        body.addView(button("Import invitation QR image") { qrPicker.launch("image/*") })
        input("contacts.search", "Search contacts").doAfterTextChanged { refreshDynamic() }
        dynamic = column().also { body.addView(it) }
    }
    private fun messages() {
        val contacts = objects(array("contacts"))
        if (contacts.isEmpty()) { hint("Add a contact to start an encrypted conversation."); body.addView(button("Add contact") { repo.screen = "Contacts"; update() }); return }
        val spinner = Spinner(this)
        spinner.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_dropdown_item, contacts.map { text(it, "alias") })
        if (repo.selected == null || contacts.none { text(it, "fingerprint") == repo.selected }) repo.selected = text(contacts.first(), "fingerprint")
        spinner.setSelection(contacts.indexOfFirst { text(it, "fingerprint") == repo.selected }.coerceAtLeast(0))
        spinner.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onNothingSelected(parent: AdapterView<*>?) {}
            override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                val fp = text(contacts[position], "fingerprint")
                if (repo.selected != fp) { repo.selected = fp; rendered = ""; update() }
            }
        }
        body.addView(spinner)
        val contact = contacts.first { text(it, "fingerprint") == repo.selected }
        hint("${text(contact, "verification")} · End-to-end encrypted messages")
        dynamic = column().also { body.addView(it) }
        val recipient = repo.selected!!
        val compose = EditText(this).apply {
            hint = "Write a message"; minLines = 2; setTextColor(ink)
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
            setText(repo.drafts[recipient].orEmpty())
            doAfterTextChanged { repo.drafts[recipient] = it.toString() }
        }
        body.addView(compose)
        body.addView(button("Send encrypted message") {
            val draft = compose.text.toString()
            if (!sending && draft.isNotBlank()) {
                sending = true
                repo.command(op("message").put("recipient", recipient).put("content", draft), "Message queued", finished = { sending = false }) {
                    if (repo.drafts[recipient] == draft) { repo.drafts.remove(recipient); if (repo.selected == recipient) compose.setText("") }
                    sending = false; repo.syncNow()
                }
            }
        })
        hint("Queued messages retry automatically. Relayed means a peer stored the envelope; it is not a read receipt.")
    }
    private fun feed() {
        input("feed.post", "Share a post", multiline = true)
        body.addView(button("Publish post") {
            val draft = repo.forms["feed.post"].orEmpty()
            repo.command(op("post").put("content", draft), "Post published") {
                if (repo.forms["feed.post"] == draft) repo.forms["feed.post"] = ""
                rendered = ""; update(); repo.syncNow()
            }
        })
        input("feed.search", "Search feed").doAfterTextChanged { refreshDynamic() }
        dynamic = column().also { body.addView(it) }
    }
    private fun network() {
        dynamic = column().also { body.addView(it) }
        body.addView(button("Sync now") { repo.syncNow() })
        body.addView(button("Pause / resume sync") { repo.command(op("pause").put("paused", !repo.state.optBoolean("paused"))) })
        body.addView(button("Toggle nearby discovery") { repo.command(op("discovery").put("enabled", !repo.state.optBoolean("discovery"))) })
        body.addView(button("Clean inactive cache files") { repo.command(op("cleanup")) { toast("Removed ${it.optInt("removed")} inactive files") } })
        hint("Sync runs while the app is open. Android may stop the process in the background; queued messages survive and retry when you return. Across networks, use a reachable IP:port or a VPN and share a fresh invitation.")
    }
    private fun refreshDynamic() {
        val parent = dynamic ?: return
        parent.removeAllViews()
        when (repo.screen) {
            "Messages" -> {
                val thread = objects(array("threads")).find { text(it, "fingerprint") == repo.selected }
                val messages = objects(thread?.optJSONArray("messages") ?: JSONArray())
                if (messages.isEmpty()) hint("No messages yet. Say hello.", parent)
                for (message in messages) {
                    val id = text(message, "id")
                    val box = column().apply {
                        setPadding(dp(12), dp(8), dp(12), dp(8))
                        setBackgroundColor(if (message.optBoolean("incoming")) Color.WHITE else Color.rgb(221, 239, 232))
                    }
                    box.addView(label(if (repo.ciphertext.contains(id)) text(message, "ciphertext") else text(message, "text", text(message, "error", "Cannot decrypt"))).apply { setTextIsSelectable(true) })
                    hint("${if (message.optBoolean("incoming")) "Received" else text(message, "delivery")} · ${text(message, "time")}", box)
                    if (message.optBoolean("encrypted")) box.addView(button(if (repo.ciphertext.contains(id)) "View plaintext" else "View ciphertext") {
                        if (!repo.ciphertext.remove(id)) repo.ciphertext.add(id); refreshDynamic()
                    })
                    parent.addView(box, LinearLayout.LayoutParams(-1, -2).apply { bottomMargin = dp(10) })
                }
                if ((thread?.optInt("unread") ?: 0) > 0 && pendingRead != repo.selected) {
                    val fp = repo.selected!!; pendingRead = fp
                    repo.command(op("read").put("recipient", fp), finished = { pendingRead = null })
                }
            }
            "Contacts" -> {
                parent.addView(label("Your contacts", 20))
                val search = repo.forms["contacts.search"].orEmpty()
                for (c in objects(array("contacts")).filter { it.toString().contains(search, true) }) {
                    parent.addView(label(text(c, "alias"), 18))
                    hint("${text(c, "verification")} · ${text(c, "fingerprint")}", parent)
                    hint(text(c, "profile_summary"), parent)
                    if (!c.isNull("last_sync_error")) hint(text(c, "last_sync_error"), parent)
                    parent.addView(button("Message ${text(c, "alias")}") { repo.selected = text(c, "fingerprint"); repo.screen = "Messages"; update() })
                }
                parent.addView(label("Nearby", 20))
                val nearby = objects(array("nearby"))
                if (nearby.isEmpty()) hint("No nearby peers. Enable discovery in Network and connect both devices to the same Wi-Fi.", parent)
                for (p in nearby) parent.addView(button("Add ${text(p, "alias")}") {
                    repo.command(op("contact").put("mode", "manual").put("input", text(p, "fingerprint")).put("alias", text(p, "alias")).put("address", text(p, "address")), "Nearby contact added") { repo.syncNow() }
                })
            }
            "Feed" -> {
                val search = repo.forms["feed.search"].orEmpty()
                parent.addView(label("Your posts", 20))
                for (signed in objects(array("posts"))) {
                    val post = signed.getJSONObject("post")
                    if (text(post, "content").contains(search, true)) { parent.addView(label(text(post, "content"))); hint(text(post, "created_at"), parent) }
                }
                parent.addView(label("From your contacts", 20))
                for (c in objects(array("contacts"))) {
                    val preview = text(c, "latest_post_preview")
                    if (preview.isNotEmpty() && (preview + text(c, "alias")).contains(search, true)) {
                        parent.addView(label(text(c, "alias"), 18)); parent.addView(label(preview)); hint("${c.optInt("synced_post_count")} synced posts", parent)
                    }
                }
            }
            "Network" -> {
                hint("Sync: ${if (repo.state.optBoolean("paused")) "paused" else "active"}\nLast sync: ${text(repo.state, "lastSync")}\nPeer endpoints: ${repo.state.optInt("peers")}\nListening: ${text(repo.state, "listening", "unavailable")}\nNearby discovery: ${if (repo.state.optBoolean("discovery")) "on" else "off"}", parent)
                if (!repo.state.isNull("listenerError")) hint(text(repo.state, "listenerError"), parent)
            }
        }
    }
    private fun setAvatar(view: ImageView, value: String) {
        if (value.isEmpty()) return
        try { val bytes = Base64.decode(value.substringAfter("base64,"), Base64.DEFAULT); view.setImageBitmap(BitmapFactory.decodeByteArray(bytes, 0, bytes.size)) } catch (_: Exception) { }
    }
    private fun loadAvatar(uri: Uri) {
        io.execute {
            try {
                val bytes = contentResolver.openInputStream(uri)?.use { stream ->
                    val data = stream.readBytesLimited(16 * 1024 * 1024); data
                } ?: error("Cannot open image")
                val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
                BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
                if (bounds.outWidth <= 0 || bounds.outHeight <= 0) error("Cannot decode image")
                val options = BitmapFactory.Options().apply { inSampleSize = (maxOf(bounds.outWidth, bounds.outHeight) / 512).coerceAtLeast(1) }
                val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options) ?: error("Cannot decode image")
                val ratio = 256.0 / maxOf(bitmap.width, bitmap.height)
                val resized = Bitmap.createScaledBitmap(bitmap, (bitmap.width * ratio).toInt().coerceAtLeast(1), (bitmap.height * ratio).toInt().coerceAtLeast(1), true)
                val output = ByteArrayOutputStream(); resized.compress(Bitmap.CompressFormat.PNG, 100, output)
                val avatar = "data:image/png;base64," + Base64.encodeToString(output.toByteArray(), Base64.NO_WRAP)
                runOnUiThread { repo.forms["profile.avatar"] = avatar; rendered = ""; update() }
            } catch (e: Exception) { runOnUiThread { toast(e.message ?: "Image import failed") } }
        }
    }
    private fun importQr(uri: Uri) {
        io.execute {
            var file: File? = null
            try {
                file = File.createTempFile("invite", ".image", cacheDir)
                contentResolver.openInputStream(uri)?.use { input -> file.writeBytes(input.readBytesLimited(16 * 1024 * 1024)) } ?: error("Cannot open image")
                val imported = file
                runOnUiThread {
                    repo.command(op("importQr").put("path", imported.absolutePath), "Invitation imported", finished = { imported.delete() }) { repo.syncNow() }
                }
            } catch (e: Exception) { file?.delete(); runOnUiThread { toast(e.message ?: "QR import failed") } }
        }
    }
    private fun java.io.InputStream.readBytesLimited(limit: Int): ByteArray {
        val out = ByteArrayOutputStream(); val buffer = ByteArray(8192)
        while (true) { val count = read(buffer); if (count < 0) break; if (out.size() + count > limit) error("Image exceeds 16 MB"); out.write(buffer, 0, count) }
        return out.toByteArray()
    }
    private fun export(format: String) {
        if (exporting) return
        exportFormat = format; exporting = true
        exporter.launch(Intent(Intent.ACTION_CREATE_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE)
            .setType(when (format) { "svg" -> "image/svg+xml"; "jpg" -> "image/jpeg"; else -> "image/png" })
            .putExtra(Intent.EXTRA_TITLE, "snartnet-invitation.$format"))
    }
}
