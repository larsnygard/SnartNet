package com.snartnet.android

/**
 * The Rust boundary. Every entry point forwards to the shared backend service that runs inside
 * this process (ADR 0001, M10.1), so the Kotlin side is a frontend and never owns state.
 */
object NativeBridge {
    init { System.loadLibrary("snartnet_android_bridge") }

    /** Start the service if needed and return the initial state snapshot. */
    external fun nativeInit(root: String): String

    /** Forward one UI request to the service. */
    external fun nativeCommand(request: String): String

    /** Run one publish/ingest/push round in the service. */
    external fun nativeSync(): String

    /**
     * Report visibility and power state so the service can pick its sync mode (M10.2):
     * on screen synchronizes, hidden while saving battery pauses entirely.
     */
    external fun nativeSetLifecycle(visible: Boolean, powerSave: Boolean, charging: Boolean): String
}
