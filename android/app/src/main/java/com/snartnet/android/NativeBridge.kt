package com.snartnet.android

object NativeBridge {
    init { System.loadLibrary("snartnet_android_bridge") }
    external fun nativeInit(root: String): String
    external fun nativeCommand(request: String): String
    external fun nativeSync(): String
}
