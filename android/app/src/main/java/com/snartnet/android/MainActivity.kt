package com.snartnet.android

import android.os.Bundle
import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import java.io.File

class MainActivity : AppCompatActivity() {
    private lateinit var output: TextView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        output = findViewById(R.id.output)
        val usernameInput = findViewById<EditText>(R.id.usernameInput)
        val displayNameInput = findViewById<EditText>(R.id.displayNameInput)
        val bioInput = findViewById<EditText>(R.id.bioInput)
        val recipientInput = findViewById<EditText>(R.id.recipientInput)
        val postInput = findViewById<EditText>(R.id.postInput)
        val messageInput = findViewById<EditText>(R.id.messageInput)
        val inviteInput = findViewById<EditText>(R.id.inviteInput)

        findViewById<Button>(R.id.btnInit).setOnClickListener {
            val dbPath = File(filesDir, "snartnet_android.db").absolutePath
            showOutput("Init", NativeBridge.nativeInit(dbPath))
        }

        findViewById<Button>(R.id.btnCreateProfile).setOnClickListener {
            val username = usernameInput.text?.toString()?.ifBlank { "android_user" } ?: "android_user"
            val displayName = displayNameInput.text?.toString().orEmpty()
            val bio = bioInput.text?.toString().orEmpty()
            showOutput("Create profile", NativeBridge.nativeCreateProfile(username, displayName, bio))
        }

        findViewById<Button>(R.id.btnUpdateProfile).setOnClickListener {
            val displayName = displayNameInput.text?.toString().orEmpty()
            val bio = bioInput.text?.toString().orEmpty()
            showOutput("Update profile", NativeBridge.nativeUpdateProfile(displayName, bio))
        }

        findViewById<Button>(R.id.btnGetProfile).setOnClickListener {
            showOutput("Get profile", NativeBridge.nativeGetProfileJson())
        }

        findViewById<Button>(R.id.btnGetFingerprint).setOnClickListener {
            showOutput("Fingerprint", NativeBridge.nativeGetFingerprint())
        }

        findViewById<Button>(R.id.btnGetPublicKey).setOnClickListener {
            showOutput("Public key", NativeBridge.nativeGetPublicKey())
        }

        findViewById<Button>(R.id.btnGetCapabilities).setOnClickListener {
            showOutput("Capabilities", NativeBridge.nativeGetCapabilities())
        }

        findViewById<Button>(R.id.btnCreatePost).setOnClickListener {
            val postText = postInput.text?.toString()?.ifBlank { "Hello from Android shell" } ?: "Hello from Android shell"
            showOutput("Create post", NativeBridge.nativeCreatePost(postText))
        }

        findViewById<Button>(R.id.btnCreateMessage).setOnClickListener {
            val recipient = recipientInput.text?.toString()?.ifBlank { "recipient-fingerprint" } ?: "recipient-fingerprint"
            val message = messageInput.text?.toString()?.ifBlank { "Hello from Android messaging" } ?: "Hello from Android messaging"
            showOutput("Create message", NativeBridge.nativeCreateMessage(recipient, message))
        }

        findViewById<Button>(R.id.btnExportInvite).setOnClickListener {
            showOutput("Export invite", NativeBridge.nativeExportInviteCode())
        }

        findViewById<Button>(R.id.btnImportInvite).setOnClickListener {
            val code = inviteInput.text?.toString().orEmpty()
            showOutput("Import invite", NativeBridge.nativeImportInviteCode(code))
        }

        findViewById<Button>(R.id.btnGenerateQr).setOnClickListener {
            showOutput("Generate invite QR", NativeBridge.nativeGenerateInviteQr())
        }

        findViewById<Button>(R.id.btnStartLan).setOnClickListener {
            showOutput("Start LAN discovery", NativeBridge.nativeStartLanDiscovery())
        }

        findViewById<Button>(R.id.btnStopLan).setOnClickListener {
            showOutput("Stop LAN discovery", NativeBridge.nativeStopLanDiscovery())
        }

        findViewById<Button>(R.id.btnLanStatus).setOnClickListener {
            showOutput("LAN status", NativeBridge.nativeGetLanDiscoveryStatus())
        }

        findViewById<Button>(R.id.btnLanPeers).setOnClickListener {
            showOutput("LAN peers", NativeBridge.nativeGetDiscoveredPeers())
        }
    }

    private fun showOutput(title: String, payload: String) {
        output.text = "$title\n$payload"
    }
}
