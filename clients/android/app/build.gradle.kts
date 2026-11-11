import groovy.json.JsonSlurper
import javax.inject.Inject

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
}

val productVersion = rootDir.resolve("../../VERSION").readText().trim()
val ghostty = rootDir.resolve("../../terminal/ghostty")
val certificateVerifier = Regex("name = \"rustls-platform-verifier-android\"\nversion = \"([^\"]+)\"")
    .find(rootDir.resolve("../../runtime/Cargo.lock").readText())?.groupValues?.get(1)
check(certificateVerifier == libs.versions.rustlsPlatformVerifier.get()) {
    "rustlsPlatformVerifier in libs.versions.toml must be $certificateVerifier, " +
        "the version of rustls-platform-verifier-android in runtime/Cargo.lock."
}
val runtimeLibraries = layout.buildDirectory.dir("runtime")
val debugBackend = providers.environmentVariable("KODOSI__BACKEND__API").orElse("http://127.0.0.1:5180")
val debugIssuer = providers.environmentVariable("KODOSI__AUTH__ISSUER").orElse("http://127.0.0.1:8284/realms/kodosi")

fun quoted(value: String) = "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"") + "\""

android {
    namespace = "com.kodosi.mobile"
    compileSdk = libs.versions.compileSdk.get().toInt()
    buildToolsVersion = libs.versions.buildTools.get()
    ndkVersion = libs.versions.ndk.get()

    defaultConfig {
        applicationId = "com.kodosi.mobile"
        minSdk = libs.versions.minSdk.get().toInt()
        targetSdk = libs.versions.targetSdk.get().toInt()
        versionName = productVersion
        versionCode = productVersion.split(".").map(String::toInt).let { (major, minor, patch) ->
            major * 1_000_000 + minor * 1_000 + patch
        }
        ndk { abiFilters += "arm64-v8a" }
        externalNativeBuild {
            cmake { arguments += "-DANDROID_STL=c++_shared" }
        }
    }

    buildTypes {
        debug {
            buildConfigField("String", "DEBUG_BACKEND_API", quoted(debugBackend.get()))
            buildConfigField("String", "DEBUG_AUTH_ISSUER", quoted(debugIssuer.get()))
            externalNativeBuild {
                cmake { arguments += "-DKODOSI_RUNTIME_DIR=${runtimeLibraries.get().dir("dev").asFile}" }
            }
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            buildConfigField("String", "DEBUG_BACKEND_API", "\"\"")
            buildConfigField("String", "DEBUG_AUTH_ISSUER", "\"\"")
            externalNativeBuild {
                cmake { arguments += "-DKODOSI_RUNTIME_DIR=${runtimeLibraries.get().dir("release").asFile}" }
            }
        }
    }

    sourceSets {
        getByName("debug") { jniLibs.directories += runtimeLibraries.get().dir("dev").asFile.path }
        getByName("release") { jniLibs.directories += runtimeLibraries.get().dir("release").asFile.path }
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = libs.versions.cmake.get()
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    androidResources { generateLocaleConfig = true }

    lint {
        warningsAsErrors = true
        abortOnError = true
        checkDependencies = false
        disable += listOf(
            "GradleDependency",
            "NewerVersionAvailable",
            "AndroidGradlePluginVersion",
            "OldTargetApi",
            "ChromeOsAbiSupport"
        )
    }

    testOptions { unitTests.isReturnDefaultValues = false }
}

abstract class CollectNotices @Inject constructor(private val files: FileSystemOperations) : DefaultTask() {
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.NAME_ONLY)
    abstract val notices: ConfigurableFileCollection

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.NAME_ONLY)
    abstract val ghosttyNotices: ConfigurableFileCollection

    @get:OutputDirectory
    abstract val output: DirectoryProperty

    @TaskAction
    fun collect() {
        files.sync {
            into(output)
            from(notices) { into("licenses") }
            from(ghosttyNotices) { into("licenses/Ghostty") }
        }
    }
}

val ghosttyComponents = JsonSlurper().parse(ghostty.resolve("ThirdPartyNotices/android-vt-inventory.json"))
    .let { (it as Map<*, *>)["components"] as List<*> }
    .flatMap { (it as Map<*, *>)["notices"] as List<*> }
    .map { ghostty.resolve("ThirdPartyNotices/licenses/$it") }
    .filter { it.isFile }

val collectNotices = tasks.register<CollectNotices>("collectNotices") {
    notices.from(rootDir.resolve("../apple/Notices").listFiles { file -> file.extension == "txt" })
    notices.from(ghostty.resolve("ThirdPartyNotices/licenses/jetbrains-mono-OFL-1.1.txt"))
    ghosttyNotices.from(ghostty.resolve("LICENSE-GHOSTTY"))
    ghosttyNotices.from(ghostty.resolve("THIRD_PARTY_NOTICES.md"))
    ghosttyNotices.from(ghosttyComponents)
    output = layout.buildDirectory.dir("generated/notices")
}

androidComponents {
    onVariants { variant ->
        variant.sources.assets?.addGeneratedSourceDirectory(collectNotices, CollectNotices::output)
    }
}

val ktlint: Configuration = configurations.create("ktlint")

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.process)
    implementation(libs.androidx.browser)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.rustls.platform.verifier)
    debugImplementation(libs.compose.ui.tooling)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)

    ktlint(libs.ktlint.cli) {
        attributes { attribute(Bundling.BUNDLING_ATTRIBUTE, objects.named(Bundling.EXTERNAL)) }
    }
}

val ktlintSources = listOf("src/**/*.kt", "*.kts", "../*.kts")

tasks.register<JavaExec>("ktlintCheck") {
    description = "Checks the Kotlin style."
    group = "verification"
    classpath = ktlint
    mainClass = "com.pinterest.ktlint.Main"
    args(ktlintSources)
}

tasks.register<JavaExec>("ktlintFormat") {
    description = "Formats the Kotlin sources."
    group = "formatting"
    classpath = ktlint
    mainClass = "com.pinterest.ktlint.Main"
    jvmArgs("--add-opens=java.base/java.lang=ALL-UNNAMED")
    args(listOf("-F") + ktlintSources)
}
