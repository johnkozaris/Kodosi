-keep interface com.kodosi.mobile.runtime.RuntimeCallbacks { *; }
-keep class com.kodosi.mobile.runtime.KeystoreSecrets { *; }
-keepclassmembers class com.kodosi.mobile.terminals.TerminalKernel {
    public static android.graphics.Bitmap decodePng(byte[]);
}
-keepclasseswithmembernames,includedescriptorclasses class * {
    native <methods>;
}
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
