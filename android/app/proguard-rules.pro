# kotlinx.serialization keeps generated serializers through its own rules.
-keepattributes *Annotation*, InnerClasses, Signature
-dontwarn org.codehaus.mojo.animal_sniffer.*

# OkHttp / Coil
-dontwarn okhttp3.**
-dontwarn okio.**
-dontwarn coil.**
