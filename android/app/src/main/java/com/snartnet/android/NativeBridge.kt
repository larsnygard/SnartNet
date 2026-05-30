package com.snartnet.android

object NativeBridge {
    init {
        System.loadLibrary("snartnet_android_bridge")
    }

    external fun nativeInit(dbPath: String): String
    external fun nativeCreateProfile(username: String, displayName: String, bio: String): String
    external fun nativeUpdateProfile(displayName: String, bio: String): String
    external fun nativeGetProfileJson(): String
    external fun nativeGetPublicKey(): String
    external fun nativeGetFingerprint(): String
    external fun nativeGetCapabilities(): String
    external fun nativeCreatePost(content: String): String
    external fun nativeCreateMessage(recipientFingerprint: String, content: String): String
    external fun nativeExportInviteCode(): String
    external fun nativeImportInviteCode(inviteCode: String): String
    external fun nativeGenerateInviteQr(): String
    external fun nativeStartLanDiscovery(): String
    external fun nativeStopLanDiscovery(): String
    external fun nativeGetLanDiscoveryStatus(): String
    external fun nativeGetDiscoveredPeers(): String
}
