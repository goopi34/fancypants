#!/usr/bin/env bash
# Prepare the generated Tauri Android project (`gen/android`) with Fancypants's
# custom MainActivity, FancypantsBleService, droidplug/jni-utils Java sources,
# AndroidManifest permissions/service declarations, and ProGuard keep rules.
set -euo pipefail

cd "$(dirname "$0")/.."

if [ ! -d "gen/android/app/src/main" ]; then
    echo "ERROR: gen/android/app/src/main not found (did cargo tauri android init run?)" >&2
    exit 1
fi

# 1. Patch AndroidManifest.xml with BLE + ForegroundService permissions and service
python3 - << 'PYEOF'
import pathlib

manifest = pathlib.Path("gen/android/app/src/main/AndroidManifest.xml")
txt = manifest.read_text()

perms = """
    <uses-permission android:name="android.permission.BLUETOOTH" android:maxSdkVersion="30" />
    <uses-permission android:name="android.permission.BLUETOOTH_ADMIN" android:maxSdkVersion="30" />
    <uses-permission android:name="android.permission.ACCESS_FINE_LOCATION" android:maxSdkVersion="30" />
    <uses-permission android:name="android.permission.BLUETOOTH_SCAN" android:usesPermissionFlags="neverForLocation" />
    <uses-permission android:name="android.permission.BLUETOOTH_CONNECT" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE_CONNECTED_DEVICE" />
    <uses-permission android:name="android.permission.POST_NOTIFICATIONS" />
    <uses-feature android:name="android.hardware.bluetooth_le" android:required="true" />
"""

service = """
        <service
            android:name=".FancypantsBleService"
            android:enabled="true"
            android:exported="false"
            android:foregroundServiceType="connectedDevice" />
    </application>"""

if "BLUETOOTH_SCAN" not in txt:
    txt = txt.replace("<application", perms + "\n    <application", 1)
if "FancypantsBleService" not in txt:
    txt = txt.replace("</application>", service, 1)

manifest.write_text(txt)
PYEOF

# 2. Copy MainActivity.kt, FancypantsBleService.kt, and droidplug/jni-utils Java sources
cp mobile/android/MainActivity.kt gen/android/app/src/main/java/io/github/goopi34/fancypants/MainActivity.kt
cp mobile/android/FancypantsBleService.kt gen/android/app/src/main/java/io/github/goopi34/fancypants/FancypantsBleService.kt
cp -r mobile/android/java/com gen/android/app/src/main/java/
cp -r mobile/android/java/io/github/gedgygedgy gen/android/app/src/main/java/io/github/

# 3. Add ProGuard keep rules so R8 does not strip JNI classes looked up from Rust
cat >> gen/android/app/proguard-rules.pro << 'EOF'
-keep class com.nonpolynomial.btleplug.android.impl.** { *; }
-keep class io.github.gedgygedgy.rust.** { *; }
-keep class io.github.goopi34.fancypants.** { *; }
EOF

echo "✓ Android project prepared in app/src-tauri/gen/android"
