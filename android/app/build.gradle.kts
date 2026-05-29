import org.gradle.internal.os.OperatingSystem

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.snartnet.android"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.snartnet.android"
        minSdk = 24
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

val hostTag = when {
    OperatingSystem.current().isWindows -> "windows-x86_64"
    OperatingSystem.current().isMacOsX -> "darwin-x86_64"
    else -> "linux-x86_64"
}

val ndkHome = providers.environmentVariable("ANDROID_NDK_HOME")
val cargoBin = if (OperatingSystem.current().isWindows) "cargo.exe" else "cargo"
val rustTarget = "aarch64-linux-android"
val rustLibName = if (OperatingSystem.current().isWindows) "snartnet_android_bridge.dll" else "libsnartnet_android_bridge.so"

val buildRustBridge by tasks.registering(Exec::class) {
    group = "build"
    description = "Build Rust Android bridge"

    val ndk = ndkHome.orNull ?: throw GradleException("ANDROID_NDK_HOME is not set")
    val toolchain = file("$ndk/toolchains/llvm/prebuilt/$hostTag/bin")
    if (!toolchain.exists()) {
        throw GradleException("NDK toolchain folder not found: $toolchain")
    }

    environment("CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER", File(toolchain, "aarch64-linux-android24-clang").absolutePath)
    environment("CC_aarch64_linux_android", File(toolchain, "aarch64-linux-android24-clang").absolutePath)
    environment("AR_aarch64_linux_android", File(toolchain, "llvm-ar").absolutePath)

    workingDir = file("${rootDir.parentFile.absolutePath}")
    commandLine(cargoBin, "build", "-p", "snartnet-android-bridge", "--target", rustTarget, "--release")
}

val copyRustBridge by tasks.registering(Copy::class) {
    dependsOn(buildRustBridge)
    from(file("${rootDir.parentFile.absolutePath}/target/$rustTarget/release/libsnartnet_android_bridge.so"))
    into(file("src/main/jniLibs/arm64-v8a"))
}

tasks.named("preBuild") {
    dependsOn(copyRustBridge)
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("com.google.android.material:material:1.12.0")
}
