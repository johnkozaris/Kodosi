-keep interface com.kodosi.mobile.runtime.RuntimeCallbacks { *; }
-keepclasseswithmembernames,includedescriptorclasses class * {
    native <methods>;
}
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
