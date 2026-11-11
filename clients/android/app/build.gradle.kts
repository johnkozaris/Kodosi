import groovy.json.JsonSlurper
import java.io.ByteArrayInputStream
import java.io.File
import java.security.MessageDigest
import java.util.zip.ZipFile
import java.util.zip.ZipInputStream
import javax.inject.Inject
import javax.xml.parsers.DocumentBuilderFactory
import org.gradle.api.artifacts.component.ModuleComponentIdentifier
import org.gradle.api.artifacts.dsl.DependencyHandler
import org.gradle.api.artifacts.result.ResolvedArtifactResult
import org.gradle.maven.MavenModule
import org.gradle.maven.MavenPomArtifact
import org.w3c.dom.Element

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
val releaseKey = listOf(
    "KODOSI_ANDROID_KEYSTORE",
    "KODOSI_ANDROID_STORE_PASSWORD",
    "KODOSI_ANDROID_KEY_ALIAS",
    "KODOSI_ANDROID_KEY_PASSWORD"
).associateWith { providers.environmentVariable(it).orNull }
check(releaseKey.values.all { it == null } || releaseKey.values.all { !it.isNullOrBlank() }) {
    "Set all four KODOSI_ANDROID signing variables, or leave all four unset for an unsigned build."
}
val firebase = providers.fileContents(layout.projectDirectory.file("google-services.json")).asText.orNull?.let {
    runCatching { JsonSlurper().parseText(it) as? Map<*, *> }.getOrNull()
        ?: error("app/google-services.json must be a Google services configuration.")
}

fun quoted(value: String) = "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"") + "\""

fun firebaseFields(packageName: String): Map<String, String> {
    val project = firebase?.get("project_info") as? Map<*, *>
    val client = (firebase?.get("client") as? List<*>)?.filterIsInstance<Map<*, *>>()?.firstOrNull {
        val info = it["client_info"] as? Map<*, *>
        val android = info?.get("android_client_info") as? Map<*, *>
        android?.get("package_name") == packageName
    }
    val info = client?.get("client_info") as? Map<*, *>
    val key = (client?.get("api_key") as? List<*>)?.filterIsInstance<Map<*, *>>()?.firstOrNull()
    val fields = mapOf(
        "FIREBASE_APPLICATION_ID" to (info?.get("mobilesdk_app_id") as? String).orEmpty(),
        "FIREBASE_API_KEY" to (key?.get("current_key") as? String).orEmpty(),
        "FIREBASE_PROJECT_ID" to (project?.get("project_id") as? String).orEmpty(),
        "FIREBASE_SENDER_ID" to (project?.get("project_number") as? String).orEmpty()
    )
    if (client == null) return fields.mapValues { "" }
    check(fields.values.all { it.isNotBlank() }) {
        "app/google-services.json must supply the Firebase app, API key, project and sender."
    }
    return fields
}

android {
    namespace = "com.kodosi.mobile"
    compileSdk = libs.versions.compileSdk.get().toInt()
    buildToolsVersion = libs.versions.buildTools.get()
    ndkVersion = libs.versions.ndk.get()

    defaultConfig {
        applicationId = "com.kodosi.mobile"
        minSdk = libs.versions.minSdk.get().toInt()
        targetSdk = libs.versions.targetSdk.get().toInt()
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        versionName = productVersion
        versionCode = productVersion.split(".").map(String::toInt).let { (major, minor, patch) ->
            major * 1_000_000 + minor * 1_000 + patch
        }
        ndk { abiFilters += "arm64-v8a" }
        externalNativeBuild {
            cmake { arguments += "-DANDROID_STL=c++_shared" }
        }
    }

    if (releaseKey.values.all { it != null }) {
        signingConfigs.create("distribution") {
            val path = File(releaseKey.getValue("KODOSI_ANDROID_KEYSTORE")!!)
            check(path.isAbsolute && path.isFile) { "KODOSI_ANDROID_KEYSTORE must name an existing absolute file." }
            storeFile = path
            storePassword = releaseKey.getValue("KODOSI_ANDROID_STORE_PASSWORD")
            keyAlias = releaseKey.getValue("KODOSI_ANDROID_KEY_ALIAS")
            keyPassword = releaseKey.getValue("KODOSI_ANDROID_KEY_PASSWORD")
        }
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
            firebaseFields("com.kodosi.mobile.debug").forEach { (name, value) ->
                buildConfigField("String", name, quoted(value))
            }
            buildConfigField("String", "DEBUG_BACKEND_API", quoted(debugBackend.get()))
            buildConfigField("String", "DEBUG_AUTH_ISSUER", quoted(debugIssuer.get()))
            externalNativeBuild {
                cmake { arguments += "-DKODOSI_RUNTIME_DIR=${runtimeLibraries.get().dir("dev").asFile}" }
            }
        }
        release {
            signingConfig = signingConfigs.findByName("distribution")
            firebaseFields("com.kodosi.mobile").forEach { (name, value) ->
                buildConfigField("String", name, quoted(value))
            }
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
        getByName("androidTest") { assets.directories += ghostty.resolve("Tests/GhosttyKitTest/Resources").path }
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
    bundle { language { enableSplit = false } }

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

    testOptions {
        unitTests.isReturnDefaultValues = false
        unitTests.isIncludeAndroidResources = true
    }
}

/** Puts the notices of the code, the font and the artwork in the app into its assets. */
abstract class CollectNotices @Inject constructor(private val files: FileSystemOperations) : DefaultTask() {
    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val documentViews: DirectoryProperty

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.NAME_ONLY)
    abstract val notices: ConfigurableFileCollection

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.NAME_ONLY)
    abstract val ghosttyNotices: ConfigurableFileCollection

    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val rustNotices: DirectoryProperty

    @get:OutputDirectory
    abstract val output: DirectoryProperty

    @TaskAction
    fun collect() {
        files.sync {
            into(output)
            from(notices) { into("licenses") }
            from(ghosttyNotices) { into("licenses/Ghostty") }
            from(rustNotices) { into("licenses/Rust") }
            from(documentViews) { into("documents") }
            from(documentViews.file("licenses.md")) { into("licenses/Documents") }
        }
    }
}

abstract class CollectJvmNotices @Inject constructor(private val files: FileSystemOperations) : DefaultTask() {
    @get:InputFiles
    @get:PathSensitive(PathSensitivity.NAME_ONLY)
    abstract val poms: ConfigurableFileCollection

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.NAME_ONLY)
    abstract val archives: ConfigurableFileCollection

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.NAME_ONLY)
    abstract val parents: ConfigurableFileCollection

    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val rustNotices: DirectoryProperty

    @get:InputDirectory
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val additionalNotices: DirectoryProperty

    @get:OutputDirectory
    abstract val output: DirectoryProperty

    @TaskAction
    fun collect() {
        files.delete { delete(output) }
        val directory = output.get().asFile.resolve("licenses/JVM").apply { mkdirs() }
        files.copy {
            from(additionalNotices)
            into(directory)
        }
        val inherited = parents.files.map(::readPom).associateBy { it.coordinate() }
        val inventory = poms.files.sortedBy { it.name }.joinToString("\n\n") { pom ->
            val project = readPom(pom)
            val licenses = project.licenses(inherited)
            val verifier = project.coordinate().startsWith("org.rustls:rustls-platform-verifier:")
            val notice = when {
                verifier -> "MIT; see rustls-platform-verifier-MIT.txt."

                licenses.any { it.text("url").contains("apache.org/licenses/LICENSE-2.0") } ->
                    "Apache-2.0; see Apache-2.0.txt."

                project.coordinate().startsWith("androidx.datastore:datastore-preferences-external-protobuf:") &&
                    licenses.any { it.text("name") == "BSD-3-Clause" } ->
                    "BSD-3-Clause; see protobuf-BSD-3-Clause.txt."

                licenses.any {
                    it.text("name") == "Android Software Development Kit License" &&
                        it.text("url") == "https://developer.android.com/studio/terms.html"
                } ->
                    "Android Software Development Kit License; see Google-Android-SDK.txt."

                else -> error("Package the declared license of ${pom.name} before adding this dependency.")
            }
            buildString {
                appendLine(project.text("name").ifBlank { pom.name.removeSuffix(".pom") })
                appendLine(pom.name.removeSuffix(".pom"))
                appendLine(project.text("url"))
                licenses.forEach { appendLine("${it.text("name")}: ${it.text("url")}") }
                if (verifier) {
                    appendLine("MIT OR Apache-2.0, from the matching Rust crate.")
                }
                append("Distributed under $notice")
            }
        }
        directory.resolve("Dependencies.txt").writeText(inventory + "\n")
        val apache = rustNotices.get().asFile.walkTopDown()
            .filter { it.isFile && it.name.contains("apache", ignoreCase = true) }
            .map { it.readText() }
            .firstOrNull { it.contains("Version 2.0") && it.contains("END OF TERMS AND CONDITIONS") }
        checkNotNull(apache) { "The runtime notices must contain the full Apache-2.0 license." }
        directory.resolve("Apache-2.0.txt").writeText(apache)
        val verifier = rustNotices.get().asFile.walkTopDown()
            .firstOrNull { it.isFile && it.name.startsWith("LICENSE-MIT-rustls-platform-verifier-android-") }
        checkNotNull(verifier) { "Package the Android certificate verifier's MIT license." }
        verifier.copyTo(directory.resolve("rustls-platform-verifier-MIT.txt"), overwrite = true)
        archives.files.sortedBy { it.name }.forEach { archive ->
            val digest = MessageDigest.getInstance("SHA-256").digest(archive.readBytes())
                .take(6).joinToString("") { "%02x".format(it) }
            val prefix = "${archive.name}-$digest"
            ZipFile(archive).use { zip ->
                zip.entries().asSequence().filterNot { it.isDirectory }.forEach { entry ->
                    if (notice(entry.name)) {
                        zip.getInputStream(entry).use {
                            directory.resolve("$prefix-${entry.name.replace('/', '_')}").writeBytes(it.readBytes())
                        }
                    } else if (entry.name == "classes.jar") {
                        val payload = zip.getInputStream(entry).use { it.readBytes() }
                        ZipInputStream(ByteArrayInputStream(payload)).use { jar ->
                            var nested = jar.nextEntry
                            while (nested != null) {
                                if (!nested.isDirectory && notice(nested.name)) {
                                    directory.resolve("$prefix-${nested.name.replace('/', '_')}")
                                        .writeBytes(jar.readBytes())
                                }
                                nested = jar.nextEntry
                            }
                        }
                    }
                }
            }
        }
    }

    fun inheritLicenses(resolver: DependencyHandler) {
        val pending = ArrayDeque(poms.files)
        val seen = mutableSetOf<String>()
        while (pending.isNotEmpty()) {
            val project = readPom(pending.removeFirst())
            if (project.children("licenses").isNotEmpty()) continue
            val parent = project.children("parent").firstOrNull() ?: continue
            if (!seen.add(parent.coordinate())) continue
            val metadata = resolver.createArtifactResolutionQuery()
                .forModule(parent.text("groupId"), parent.text("artifactId"), parent.text("version"))
                .withArtifacts(MavenModule::class.java, MavenPomArtifact::class.java).execute()
            val component = metadata.resolvedComponents.singleOrNull()
            checkNotNull(component) { "Could not read the license parent ${parent.coordinate()}." }
            val artifact = component.getArtifacts(MavenPomArtifact::class.java).single()
            check(artifact is ResolvedArtifactResult) { "Could not read the license parent ${parent.coordinate()}." }
            parents.from(artifact.file)
            pending.add(artifact.file)
        }
    }

    private fun readPom(file: java.io.File): Element = DocumentBuilderFactory.newInstance().apply {
        setFeature("http://apache.org/xml/features/disallow-doctype-decl", true)
        isExpandEntityReferences = false
    }.newDocumentBuilder().parse(file).documentElement

    private fun Element.coordinate(): String {
        val parent = children("parent").firstOrNull()
        return listOf(
            text("groupId").ifBlank { parent?.text("groupId").orEmpty() },
            text("artifactId"),
            text("version").ifBlank { parent?.text("version").orEmpty() }
        ).joinToString(":")
    }

    private fun Element.licenses(inherited: Map<String, Element>, seen: Set<String> = emptySet()): List<Element> {
        check(coordinate() !in seen && seen.size < 16) { "Cyclic license inheritance in ${coordinate()}." }
        val own = children("licenses").flatMap { it.children("license") }
        if (own.isNotEmpty()) return own
        val parent = children("parent").firstOrNull()?.coordinate() ?: return emptyList()
        return inherited[parent]?.licenses(inherited, seen + coordinate()).orEmpty()
    }

    private fun notice(name: String): Boolean =
        Regex("(^|/)(license|notice|copying)([.-][^/]*)?$", RegexOption.IGNORE_CASE).containsMatchIn(name)

    private fun Element.children(name: String): List<Element> =
        (0 until childNodes.length).mapNotNull { childNodes.item(it) as? Element }.filter { it.tagName == name }

    private fun Element.text(name: String): String = children(name).firstOrNull()?.textContent?.trim().orEmpty()
}

val ghosttyComponents = JsonSlurper().parse(ghostty.resolve("ThirdPartyNotices/android-vt-inventory.json"))
    .let { (it as Map<*, *>)["components"] as List<*> }
    .flatMap { (it as Map<*, *>)["notices"] as List<*> }
    .map { ghostty.resolve("ThirdPartyNotices/licenses/$it") }
    .filter { it.isFile }

val collectNotices = tasks.register<CollectNotices>("collectNotices") {
    documentViews = layout.buildDirectory.dir("document-views")
    notices.from(rootDir.resolve("../apple/Notices").listFiles { file -> file.extension == "txt" })
    notices.from(ghostty.resolve("ThirdPartyNotices/licenses/jetbrains-mono-OFL-1.1.txt"))
    ghosttyNotices.from(ghostty.resolve("LICENSE-GHOSTTY"))
    ghosttyNotices.from(ghostty.resolve("THIRD_PARTY_NOTICES.md"))
    ghosttyNotices.from(ghosttyComponents)
    rustNotices = layout.buildDirectory.dir("rust-notices")
    output = layout.buildDirectory.dir("generated/notices")
}

androidComponents {
    onVariants { variant ->
        variant.sources.assets?.addGeneratedSourceDirectory(collectNotices, CollectNotices::output)
        val jvmNotices = tasks.register<CollectJvmNotices>(
            "collect${variant.name.replaceFirstChar(Char::titlecase)}JvmNotices"
        ) {
            val runtime = configurations.getByName("${variant.name}RuntimeClasspath")
            val components = runtime.incoming.resolutionResult.allComponents
                .map { it.id }.filterIsInstance<ModuleComponentIdentifier>()
            val metadata = dependencies.createArtifactResolutionQuery().forComponents(components)
                .withArtifacts(MavenModule::class.java, MavenPomArtifact::class.java).execute()
            check(metadata.components.size == metadata.resolvedComponents.size) {
                "Could not resolve dependency license metadata."
            }
            poms.from(
                metadata.resolvedComponents.flatMap { component ->
                    component.getArtifacts(MavenPomArtifact::class.java).map { artifact ->
                        check(artifact is ResolvedArtifactResult) { "Could not read the license of ${component.id}." }
                        artifact.file
                    }
                }
            )
            inheritLicenses(dependencies)
            archives.from(runtime)
            rustNotices = layout.buildDirectory.dir("rust-notices")
            additionalNotices = layout.projectDirectory.dir("../notices")
            output = layout.buildDirectory.dir("generated/jvm-notices/${variant.name}")
        }
        variant.sources.assets?.addGeneratedSourceDirectory(jvmNotices, CollectJvmNotices::output)
    }
}

val ktlint: Configuration = configurations.create("ktlint")

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(platform(libs.firebase.bom))
    implementation(libs.firebase.messaging)
    implementation(libs.androidx.work.runtime)
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.compose.navigation.suite)
    implementation(libs.androidx.navigation3.runtime)
    implementation(libs.androidx.navigation3.ui)
    implementation(libs.androidx.webkit)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.browser)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.kotlinx.coroutines.play.services)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.rustls.platform.verifier)
    debugImplementation(libs.compose.ui.tooling)

    testImplementation(libs.junit)
    testImplementation(libs.robolectric)
    testImplementation(libs.kotlinx.coroutines.test)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.junit)

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
