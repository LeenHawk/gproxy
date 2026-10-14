import java.util.Properties
import org.gradle.process.ExecOperations
import com.android.build.gradle.internal.tasks.R8Task
import com.google.gson.Gson
import com.google.gson.JsonParser

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// Keep ABI flavors unchanged: Tauri generates and invokes their Gradle tasks.
// The same build environment also removes the native updater in build.rs.
val distribution = providers.environmentVariable("GPROXY_ANDROID_DISTRIBUTION").getOrElse("direct")
require(distribution in listOf("direct", "fdroid", "google-play", "appgallery")) {
    "Unknown GPROXY_ANDROID_DISTRIBUTION: $distribution"
}
val selfUpdate = distribution == "direct"
val privacyDir = rootProject.file("../../../../distribution/mobile/privacy")

// Wry 0.57 generates deprecated WebSQL and back-navigation calls. Compile
// corrected copies without editing Cargo's generated inputs (which would
// invalidate Wry's build script on every subsequent build).
val wryPackagePath = "com/leenhawk/gproxy/desktop/generated"
val prepareWrySources = tasks.register<Sync>("prepareWrySources") {
    filteringCharset = "UTF-8"
    from(layout.projectDirectory.dir("src/main/java/$wryPackagePath")) {
        include("*.kt")
        filter { line: String ->
            when (line.trim()) {
                "settings.databaseEnabled = true" -> "" // WebSQL; DOM storage stays enabled.
                "this@WryActivity.onBackPressed()" ->
                    line.replace(".onBackPressed()", ".onBackPressedDispatcher.onBackPressed()")
                else -> line
            }
        }
    }
    into(layout.buildDirectory.dir("generated/wry"))
}

tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile>().configureEach {
    dependsOn(prepareWrySources)
    exclude("$wryPackagePath/**")
}

android {
    sourceSets.getByName("main").java.srcDir(layout.buildDirectory.dir("generated/wry"))
    compileSdk = 36
    buildToolsVersion = "36.1.0"
    ndkVersion = "30.0.15729638"
    providers.environmentVariable("ANDROID_NDK_HOME").orNull?.let { ndkPath = it }
    namespace = "com.leenhawk.gproxy.desktop"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "com.leenhawk.gproxy.desktop"
        buildConfigField("boolean", "SELF_UPDATE", selfUpdate.toString())
        // Tauri's template says 24. 28 is what v3's APK shipped, and it is
        // what the foreground service wants: notification channels, typed
        // foreground services and `canRequestPackageInstalls` all arrived by
        // 26, and Android 9 is old enough that going below it would mean
        // carrying compatibility branches for devices that cannot run a
        // modern WebView anyway.
        minSdk = 28
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    val uploadKeystore = providers.environmentVariable("GPROXY_ANDROID_KEYSTORE").orNull
    if (!selfUpdate && distribution != "fdroid" && uploadKeystore != null) {
        val upload = signingConfigs.create("storeUpload") {
            storeFile = file(uploadKeystore)
            storePassword = providers.environmentVariable("GPROXY_ANDROID_STORE_PASSWORD").get()
            keyAlias = providers.environmentVariable("GPROXY_ANDROID_KEY_ALIAS").get()
            keyPassword = providers.environmentVariable("GPROXY_ANDROID_KEY_PASSWORD")
                .getOrElse(storePassword!!)
        }
        buildTypes.getByName("release").signingConfig = upload
    }
    buildFeatures {
        buildConfig = true
    }
    if (!selfUpdate) {
        // Merge removals after the main manifest, for debug and release alike.
        sourceSets.getByName("debug").manifest.srcFile("src/store/AndroidManifest.xml")
        sourceSets.getByName("release").manifest.srcFile("src/store/AndroidManifest.xml")
        sourceSets.getByName("main").assets.srcDir(privacyDir)
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")

androidComponents {
    onVariants(selector().withBuildType("release")) { variant ->
        variant.packaging.jniLibs.useLegacyPackaging.set(true)
        variant.packaging.jniLibs.useLegacyPackagingFromBundle.set(true)
    }
}

val execOperations = objects.newInstance<AndroidExecServices>().execOperations

abstract class AndroidExecServices {
    @get:javax.inject.Inject
    abstract val execOperations: ExecOperations
}

// Pack the staged libraries after AGP's strip step, before APK/AAB assembly
// and signing. Applying this to the native task covers every release channel.
val nativePacker = rootProject.file("../../../../scripts/pack-mobile-native.py")
tasks.configureEach {
    if (name.startsWith("strip") && name.endsWith("ReleaseDebugSymbols")) {
        inputs.file(nativePacker)
        inputs.file(rootProject.file("../../../../scripts/install-android-upx.sh"))
        inputs.file(rootProject.file("../../../../scripts/install-android-upx.patch"))
        doLast {
            val ndk = providers.environmentVariable("ANDROID_NDK_HOME").get()
            val strip = fileTree("$ndk/toolchains/llvm/prebuilt") {
                include("*/bin/llvm-strip")
            }.singleFile
            outputs.files.files.filter { it.isDirectory }.forEach { directory ->
                execOperations.exec {
                    commandLine("python3", nativePacker.absolutePath,
                        directory.absolutePath, strip.absolutePath, "--android")
                }
            }
        }
    }
}

// AGP 8.11 includes elapsed R8 execution time in AAB diagnostics. Preserve the
// optimization metadata and DEX checksums, but omit this non-reproducible metric
// before bundling/signing. It does not affect R8 optimization or app bytecode.
tasks.withType<R8Task>().configureEach {
    doLast {
        val file = r8Metadata.get().asFile
        val metadata = JsonParser.parseString(file.readText()).asJsonObject
        metadata.getAsJsonObject("compilation").remove("buildTimeNs")
        file.writeText(Gson().toJson(metadata))
    }
}
