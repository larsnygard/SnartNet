pluginManagement {
    resolutionStrategy {
        eachPlugin {
            val version = requested.version ?: return@eachPlugin
            when (requested.id.id) {
                "com.android.application",
                "com.android.library",
                "com.android.test" -> useModule("com.android.tools.build:gradle:$version")
                "org.jetbrains.kotlin.android",
                "org.jetbrains.kotlin.jvm",
                "org.jetbrains.kotlin.multiplatform" -> useModule("org.jetbrains.kotlin:kotlin-gradle-plugin:$version")
            }
        }
    }

    repositories {
        val configuredMirrorUrls = providers.gradleProperty("snartnetGoogleMirrorUrls").orNull
            ?: System.getenv("SNARTNET_GOOGLE_MIRROR_URLS")
        val mirrorUrls = if (configuredMirrorUrls.isNullOrBlank()) {
            listOf("https://maven.aliyun.com/repository/google")
        } else {
            configuredMirrorUrls.split(",").mapNotNull { it.trim().takeIf(String::isNotEmpty) }
        }
        mirrorUrls.forEach { mirrorUrl -> maven(url = uri(mirrorUrl)) }
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        val configuredMirrorUrls = providers.gradleProperty("snartnetGoogleMirrorUrls").orNull
            ?: System.getenv("SNARTNET_GOOGLE_MIRROR_URLS")
        val mirrorUrls = if (configuredMirrorUrls.isNullOrBlank()) {
            listOf("https://maven.aliyun.com/repository/google")
        } else {
            configuredMirrorUrls.split(",").mapNotNull { it.trim().takeIf(String::isNotEmpty) }
        }
        mirrorUrls.forEach { mirrorUrl -> maven(url = uri(mirrorUrl)) }
        google()
        mavenCentral()
    }
}

rootProject.name = "SnartNetAndroid"
include(":app")
