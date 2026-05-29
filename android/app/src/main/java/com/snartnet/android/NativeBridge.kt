package com.snartnet.android

object NativeBridge {
    init {
        System.loadLibrary("snartnet_android_bridge")
    }

    external fun nativeInit(dbPath: String): String
    external fun nativeCreateProfile(username: String, displayName: String, bio: String): String
    external fun nativeGetProfileJson(): String
    external fun nativeCreatePost(content: String): String
    external fun nativeCreateMessage(recipientFingerprint: String, content: String): String
}
