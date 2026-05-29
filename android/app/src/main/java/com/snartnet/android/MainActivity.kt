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
        val recipientInput = findViewById<EditText>(R.id.recipientInput)

        val initBtn = findViewById<Button>(R.id.btnInit)
        val profileBtn = findViewById<Button>(R.id.btnCreateProfile)
        val getProfileBtn = findViewById<Button>(R.id.btnGetProfile)
        val postBtn = findViewById<Button>(R.id.btnCreatePost)
        val msgBtn = findViewById<Button>(R.id.btnCreateMessage)

        initBtn.setOnClickListener {
            val dbPath = File(filesDir, "snartnet_android.db").absolutePath
            output.text = NativeBridge.nativeInit(dbPath)
        }

        profileBtn.setOnClickListener {
            val username = usernameInput.text?.toString()?.ifBlank { "android_user" } ?: "android_user"
            output.text = NativeBridge.nativeCreateProfile(username, "", "")
        }

        getProfileBtn.setOnClickListener {
            output.text = NativeBridge.nativeGetProfileJson()
        }

        postBtn.setOnClickListener {
            output.text = NativeBridge.nativeCreatePost("Hello from Android shell")
        }

        msgBtn.setOnClickListener {
            val recipient = recipientInput.text?.toString()?.ifBlank { "recipient-fingerprint" } ?: "recipient-fingerprint"
            output.text = NativeBridge.nativeCreateMessage(recipient, "Hello from Android messaging")
        }
    }
}
