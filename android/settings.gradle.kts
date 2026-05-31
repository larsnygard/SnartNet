import org.gradle.api.artifacts.dsl.RepositoryHandler

val googleMirrorUrls = run {
    val configured = providers.gradleProperty("snartnetGoogleMirrorUrls").orNull
        ?: System.getenv("SNARTNET_GOOGLE_MIRROR_URLS")
    if (configured.isNullOrBlank()) {
        listOf("https://maven.aliyun.com/repository/google")
    } else {
        configured.split(",").mapNotNull { it.trim().takeIf(String::isNotEmpty) }
    }
}

fun RepositoryHandler.googleWithMirrors(mirrorUrls: List<String>) {
    mirrorUrls.forEach { mirrorUrl -> maven(url = uri(mirrorUrl)) }
    google()
}

pluginManagement {
    repositories {
        googleWithMirrors(googleMirrorUrls)
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        googleWithMirrors(googleMirrorUrls)
        mavenCentral()
    }
}

rootProject.name = "SnartNetAndroid"
include(":app")
