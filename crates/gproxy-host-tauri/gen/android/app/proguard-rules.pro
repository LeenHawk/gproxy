# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# The JNI seam. `Java_com_leenhawk_gproxy_desktop_GproxyNative_nativeStart` and its
# three siblings are symbols in `libgproxy_host_tauri.so`, and the runtime
# finds them by deriving that name from the *Kotlin* class and method names.
# R8 renaming either one leaves the symbol unfindable and the app dead at the
# first call, in release builds only — which is the worst possible moment to
# find out.
#
# `proguard-android-optimize.txt` already carries a general
# `-keepclasseswithmembernames class * { native <methods>; }`, so this is
# belt and braces. It is spelled out anyway because the general rule is a
# default in somebody else's file, and this crate's correctness should not
# depend on that file keeping it.
-keep class com.leenhawk.gproxy.desktop.GproxyNative { *; }
-keepclasseswithmembernames class * {
    native <methods>;
}

# The four components the manifest names by class. R8 keeps manifest-declared
# components on its own; these are stated for the same reason as above.
-keep class com.leenhawk.gproxy.desktop.GproxyService { *; }
-keep class com.leenhawk.gproxy.desktop.GproxyBootReceiver { *; }
-keep class com.leenhawk.gproxy.desktop.GproxyUpdateActivity { *; }
-keep class com.leenhawk.gproxy.desktop.GproxyUpdateProvider { *; }

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile
