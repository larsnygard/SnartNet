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
        versionCode = 2
        versionName = "0.3.2"
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

    ndkVersion = "27.0.12077973"
}

val hostTag = when {
    OperatingSystem.current().isWindows -> "windows-x86_64"
    OperatingSystem.current().isMacOsX -> "darwin-x86_64"
    else -> "linux-x86_64"
}

val cargoBin = if (OperatingSystem.current().isWindows) "cargo.exe" else "cargo"
// ARM64 phones by default; opt into x86_64 for emulator development.
val rustAbis = providers.gradleProperty("snartnetAbis").orElse("arm64-v8a").get().split(",")
val rustTargets = mapOf("arm64-v8a" to "aarch64-linux-android", "x86_64" to "x86_64-linux-android")
val bridgeCopies = rustAbis.map { abi ->
    val rustTarget = rustTargets[abi] ?: throw GradleException("Unsupported ABI: $abi")
    val suffix = abi.replace("-", "").replace("_", "")
    val buildBridge = tasks.register<Exec>("buildRustBridge$suffix") {
        group = "build"
        description = "Build Rust Android bridge for $abi"
        val ndk = providers.environmentVariable("ANDROID_NDK_HOME").orNull ?: android.ndkDirectory.absolutePath
        val toolchain = file("$ndk/toolchains/llvm/prebuilt/$hostTag/bin")
        if (!toolchain.exists()) throw GradleException("NDK toolchain folder not found: $toolchain")
        val clang = File(toolchain, "${rustTarget}24-clang").absolutePath
        environment("CARGO_TARGET_${rustTarget.uppercase().replace("-", "_")}_LINKER", clang)
        environment("CC_${rustTarget.replace("-", "_")}", clang)
        environment("AR_${rustTarget.replace("-", "_")}", File(toolchain, "llvm-ar").absolutePath)
        // Native libraries must also load on Android devices with 16 KB pages.
        environment("CARGO_TARGET_${rustTarget.uppercase().replace("-", "_")}_RUSTFLAGS", "-C link-arg=-Wl,-z,max-page-size=16384")
        workingDir = rootDir.parentFile
        commandLine(cargoBin, "build", "-p", "snartnet-android-bridge", "--target", rustTarget, "--release")
    }
    tasks.register<Copy>("copyRustBridge$suffix") {
        dependsOn(buildBridge)
        from(file("${rootDir.parentFile.absolutePath}/target/$rustTarget/release/libsnartnet_android_bridge.so"))
        into(layout.buildDirectory.dir("generated/jniLibs/$abi"))
    }
}
android.sourceSets.getByName("main").jniLibs.setSrcDirs(listOf(layout.buildDirectory.dir("generated/jniLibs")))
val copyRustBridge by tasks.registering {
    dependsOn(bridgeCopies)
}

tasks.named("preBuild") {
    dependsOn(copyRustBridge)
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("com.google.android.material:material:1.12.0")
}
