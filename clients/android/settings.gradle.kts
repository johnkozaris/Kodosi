pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode = RepositoriesMode.FAIL_ON_PROJECT_REPOS
    repositories {
        google()
        mavenCentral()
        exclusiveContent {
            forRepository {
                maven(
                    "https://github.com/rustls/rustls-platform-verifier/raw/maven-archive/android-release-support/maven/"
                )
            }
            filter { includeGroup("org.rustls") }
        }
    }
}

rootProject.name = "Kodosi"
include(":app")
