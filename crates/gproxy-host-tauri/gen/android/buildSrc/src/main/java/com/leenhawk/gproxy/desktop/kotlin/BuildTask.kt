import java.io.File
import org.apache.tools.ant.taskdefs.condition.Os
import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.logging.LogLevel
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.TaskAction

open class BuildTask : DefaultTask() {
    companion object {
        /// The Tauri CLI that Gradle calls back into to run `cargo build` for
        /// one Android ABI. Pinned in the crate's `package.json` rather than
        /// taken from `PATH`, so a Gradle build and a `pnpm android:build` are
        /// the same version of the tool.
        const val CLI_SCRIPT = "node_modules/@tauri-apps/cli/tauri.js"
    }

    @Input
    var rootDirRel: String? = null
    @Input
    var target: String? = null
    @Input
    var release: Boolean? = null

    @TaskAction
    fun assemble() {
        if (System.getenv("GPROXY_ANDROID_REUSE_NATIVE") == "1") {
            val abi = when (target) {
                "aarch64" -> "arm64-v8a"
                "x86_64" -> "x86_64"
                else -> throw GradleException("Unsupported prebuilt Android target: $target")
            }
            if (!project.file("src/main/jniLibs/$abi/libgproxy_host_tauri.so").isFile) {
                throw GradleException("Missing prebuilt application library for $abi")
            }
            return
        }
        val executable = """node""";
        try {
            runTauriCli(executable)
        } catch (e: Exception) {
            if (Os.isFamily(Os.FAMILY_WINDOWS)) {
                // Try different Windows-specific extensions
                val fallbacks = listOf(
                    "$executable.exe",
                    "$executable.cmd",
                    "$executable.bat",
                )

                var lastException: Exception = e
                for (fallback in fallbacks) {
                    try {
                        runTauriCli(fallback)
                        return
                    } catch (fallbackException: Exception) {
                        lastException = fallbackException
                    }
                }
                throw lastException
            } else {
                throw e;
            }
        }
    }

    fun runTauriCli(executable: String) {
        val rootDirRel = rootDirRel ?: throw GradleException("rootDirRel cannot be null")
        val target = target ?: throw GradleException("target cannot be null")
        val release = release ?: throw GradleException("release cannot be null")
        // PATCHED, and it has to be re-applied if `tauri android init` is ever
        // re-run. The generator writes `listOf("tauri", …)` here, which makes
        // the call `node tauri …` and fails: there is no file called `tauri`
        // in the crate. The CLI is pinned as a dev dependency in
        // `package.json`, so name the script it installs. `workingDir` below
        // is the crate root, which is where `node_modules` is.
        val args = listOf(CLI_SCRIPT, "android", "android-studio-script");

        project.exec {
            workingDir(File(project.projectDir, rootDirRel))
            executable(executable)
            args(args)
            if (project.logger.isEnabled(LogLevel.DEBUG)) {
                args("-vv")
            } else if (project.logger.isEnabled(LogLevel.INFO)) {
                args("-v")
            }
            if (release) {
                args("--release")
            }
            args(listOf("--target", target))
        }.assertNormalExitValue()
    }
}
