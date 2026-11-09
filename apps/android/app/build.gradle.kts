import java.security.MessageDigest
import java.util.zip.ZipFile
import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.google.services) apply false
}

// Private deployments opt in as one tuple; never silently mix account providers.
val deployment = Properties().apply {
    val local = rootProject.file("codync.local.properties")
    if (local.exists()) local.inputStream().use(::load)
}
fun deploymentValue(name: String): String? = providers.gradleProperty(name).orNull ?: deployment.getProperty(name)
val accountOverrides = listOf("codyncCloudUrl", "codyncClerkPublishableKey", "codyncPushRelayUrl")
check(accountOverrides.none { deploymentValue(it) != null } || accountOverrides.all { deploymentValue(it) != null }) {
    "Override cloud, Clerk and push relay together in codync.local.properties or Gradle properties."
}
fun accountValue(variant: String, key: String): String {
    val plist = rootProject.file("../shared/Config/$variant.plist").readText()
    return requireNotNull(Regex("<key>$key</key>\\s*<string>([^<]*)</string>").find(plist)?.groupValues?.get(1)) {
        "Missing $key in the shared $variant account configuration."
    }
}
val upstreamRelay = rootProject.file("../ios/Kit/Sources/CodyncKit/Client/SharedStore.swift").readLines()
    .first { it.contains("static let relayURL =") }.substringAfter('"').substringBefore('"')
val firebaseConfig = deploymentValue("codyncGoogleServicesFile")?.takeIf(String::isNotBlank)?.let(::file)
check(firebaseConfig == null || firebaseConfig.isFile) { "The configured Firebase file does not exist." }
if (firebaseConfig != null) apply(plugin = "com.google.gms.google-services")

val productVersion = rootProject.file("../project.yml").readLines()
    .first { it.trimStart().startsWith("MARKETING_VERSION:") }
    .substringAfter(':').trim()
// Play needs a rising integer per upload; deriving it from the version needs no counter, and
// tools/play-submit.py reads a track's version back from it.
val productVersionCode = productVersion.split('.').map(String::toInt)
    .let { (major, minor, patch) -> major * 1_000_000 + minor * 1_000 + patch }
// Play upload key (Google re-signs with the app signing key); without it release artifacts stay unsigned.
val uploadKeystore = deploymentValue("codyncUploadKeystore")?.takeIf(String::isNotBlank)?.let(::file)
check(uploadKeystore == null || uploadKeystore.isFile) { "The configured upload keystore does not exist." }
// Explicit, emulator-only R8 acceptance, signed with the debug key.
val releaseTests = providers.gradleProperty("codyncReleaseTests").map(String::toBoolean).getOrElse(false)

val voiceModel = configurations.create("voiceModel") {
    isCanBeConsumed = false
    isCanBeResolved = true
}
val voiceAssets = layout.buildDirectory.dir("generated/voiceAssets")
@CacheableTask
abstract class VoiceModelAssets : DefaultTask() {
    @get:InputFiles @get:PathSensitive(PathSensitivity.NONE) abstract val archives: ConfigurableFileCollection
    @get:Input abstract val expectedHash: Property<String>
    @get:OutputDirectory abstract val assetDirectory: DirectoryProperty
    @TaskAction fun generate() {
        val archive = archives.singleFile
        val digest = MessageDigest.getInstance("SHA-256")
        archive.inputStream().use { input ->
            val buffer = ByteArray(1024 * 1024)
            while (true) { val count = input.read(buffer); if (count < 0) break; digest.update(buffer, 0, count) }
        }
        check(digest.digest().joinToString("") { "%02x".format(it) } == expectedHash.get()) {
            "The pinned offline voice model's integrity check failed."
        }
        val root = assetDirectory.get().asFile
        root.deleteRecursively()
        val destination = root.resolve("voice").toPath().normalize()
        ZipFile(archive).use { zip ->
            for (entry in zip.entries()) {
                val output = destination.resolve(entry.name).normalize()
                check(output.startsWith(destination)) { "Invalid voice model archive path." }
                if (entry.isDirectory) output.toFile().mkdirs()
                else {
                    output.parent.toFile().mkdirs()
                    zip.getInputStream(entry).use { input -> output.toFile().outputStream().use { input.copyTo(it) } }
                }
            }
        }
    }
}
val prepareVoiceModel = tasks.register<VoiceModelAssets>("prepareVoiceModel") {
    archives.from(voiceModel)
    expectedHash.set("30f26242c4eb449f948e42cb302dd7a686cb29a3423a8367f99ff41780942498")
    assetDirectory.set(voiceAssets)
}

fun javaString(value: String): String = "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"")
    .replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t") + "\""

android {
    namespace = "com.codync.android"
    compileSdk = 37
    defaultConfig {
        applicationId = deploymentValue("codyncApplicationId") ?: "com.pokai.Codync.android"
        minSdk = 29
        targetSdk = 36
        versionCode = productVersionCode
        versionName = productVersion
        testInstrumentationRunner = if (releaseTests) "com.codync.android.VoiceReleaseRunner" else "androidx.test.runner.AndroidJUnitRunner"
        buildConfigField("String", "PUSH_RELAY_URL", javaString(deploymentValue("codyncPushRelayUrl") ?: upstreamRelay))
        buildConfigField("boolean", "PUSH_CONFIGURED", (firebaseConfig != null).toString())
    }
    if (uploadKeystore != null) signingConfigs.create("upload") {
        storeFile = uploadKeystore
        storePassword = requireNotNull(deploymentValue("codyncUploadKeystorePassword")) { "Set codyncUploadKeystorePassword." }
        keyAlias = "upload"
        keyPassword = storePassword
    }
    buildTypes {
        debug {
            applicationIdSuffix = ".dev"
            versionNameSuffix = "-dev"
            resValue("string", "app_name", deploymentValue("codyncAppName") ?: "Codync Dev")
            manifestPlaceholders["urlScheme"] = "codync-dev"
            buildConfigField("String", "URL_SCHEME", "\"codync-dev\"")
            buildConfigField("String", "CLOUD_URL", javaString(deploymentValue("codyncCloudUrl") ?: accountValue("dev", "cloudURL")))
            buildConfigField("String", "CLERK_PUBLISHABLE_KEY", javaString(deploymentValue("codyncClerkPublishableKey") ?: accountValue("dev", "clerkPublishableKey")))
        }
        release {
            resValue("string", "app_name", deploymentValue("codyncAppName") ?: "Codync")
            manifestPlaceholders["urlScheme"] = "codync"
            buildConfigField("String", "URL_SCHEME", "\"codync\"")
            buildConfigField("String", "CLOUD_URL", javaString(deploymentValue("codyncCloudUrl") ?: accountValue("main", "cloudURL")))
            buildConfigField("String", "CLERK_PUBLISHABLE_KEY", javaString(deploymentValue("codyncClerkPublishableKey") ?: accountValue("main", "clerkPublishableKey")))
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("upload")
        }
        create("releaseAcceptance") {
            initWith(getByName("release"))
            applicationIdSuffix = ".dev"
            versionNameSuffix = "-r8-test"
            signingConfig = signingConfigs.getByName("debug")
            matchingFallbacks += "release"
            proguardFiles("release-acceptance.pro")
        }
    }
    if (releaseTests) testBuildType = "releaseAcceptance"
    buildFeatures.compose = true
    buildFeatures.buildConfig = true
    buildFeatures.resValues = true
    sourceSets.getByName("main").assets.srcDir(rootProject.file("../ios/Kit/Sources/CodyncKit/Resources/Languages"))
    sourceSets.getByName("androidTest").assets.srcDir(rootProject.file("../../docs/reference/fixtures"))
    compileOptions {
        isCoreLibraryDesugaringEnabled = true
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}
androidComponents.beforeVariants(androidComponents.selector().withBuildType("releaseAcceptance")) { it.enable = releaseTests }
androidComponents.onVariants { variant ->
    variant.sources.assets?.addGeneratedSourceDirectory(prepareVoiceModel) { task -> task.assetDirectory }
}
afterEvaluate {
    if (firebaseConfig != null) tasks.withType<com.google.gms.googleservices.GoogleServicesTask>().configureEach {
        googleServicesJsonFiles.set(listOf(firebaseConfig))
    }
}
kotlin.compilerOptions.jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)

dependencies {
    add(voiceModel.name, "com.alphacephei.models:vosk-model-small-en-us:0.15@zip")
    implementation(libs.vosk) { exclude(group = "net.java.dev.jna", module = "jna") }
    implementation("net.java.dev.jna:jna:${libs.versions.jna.get()}@aar")
    coreLibraryDesugaring(libs.desugar)
    implementation(libs.commonmark)
    implementation(libs.commonmark.tables)
    implementation(libs.commonmark.strikethrough)
    implementation(libs.coil.compose)
    implementation(libs.coil.network)
    implementation(libs.clerk)
    implementation(platform(libs.firebase.bom))
    implementation(libs.firebase.messaging)
    implementation(libs.work)
    // What Google Play has for this app (the update notice).
    implementation(libs.play.app.update)
    implementation(libs.glance)
    // Remote screen video and OpenAI Realtime voice, as the iPhone app's WebRTC.
    implementation(libs.webrtc)
    implementation(libs.camera.core)
    implementation(libs.camera.camera2)
    implementation(libs.camera.lifecycle)
    implementation(libs.camera.view)
    implementation(project(":core"))
    implementation(project(":design"))
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.compose)
    implementation(libs.lifecycle.viewmodel)
    implementation(libs.coroutines.android)
    implementation(libs.serialization.json)
    debugImplementation(libs.compose.tooling)
    debugImplementation(libs.compose.test.manifest)
    testImplementation(libs.junit)
    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.test)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.junit)
}
