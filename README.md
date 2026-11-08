# Pocket-Native Forge

**Turn 3DS games into native builds.**

Pocket-Native Forge is an Android application that generates and compiles
native ARM64 builds of Nintendo 3DS games directly on the device, using
Pocket Native's recompilation technology.

## Status

- On-device generation and compilation working.
- Android ARM64 shared-library linking working.
- Generated packages successfully imported into Pocket Native.
- Tested on Retroid Pocket Duo Lite.
- Tested with Yoshi & Poochy's Woolly World and Super Mario 3D Land.

This project is experimental. Compatibility and performance may vary by game.

## How it works

1. Select a 3DS game image (`.3ds`).
2. Optionally provide `shared_font.bin`.
3. Generate native C source using Pocket Native's `recomp3ds`.
4. Compile the generated source on the Android device using bundled Clang.
5. Link an Android ARM64 shared library.
6. Export a ZIP package for Pocket Native.

The generated package contains:

- `game.3ds`
- `title.so`
- Optional `sysdata/shared_font.bin`

## Requirements

- Android 10 or later (API 29+).
- Android SDK Platform 36.
- Java 17.
- Gradle 9.8.2 (tested).
- Android Gradle Plugin 9.3.0.
- Rust toolchain for native development.

## Source checkout

Clone with submodules:

    git clone --recurse-submodules https://github.com/hawktocomet/pocket-native-forge.git
    cd pocket-native-forge

If already cloned without submodules:

    git submodule update --init --recursive

## Android toolchain assets

The Android ARM64 Clang/LLD toolchain is distributed separately from Git
because of its size.

The complete directory must be placed at:

    android/app/src/main/assets/toolchain/
    ├── bin/
    │   ├── clang
    │   └── lld
    ├── lib/
    │   └── clang/22/
    └── sysroot/

The archive must contain the complete `toolchain` directory, including
Clang resources and the Android sysroot.

Release asset: `pn-forge-android-arm64-toolchain.tar.gz`

SHA-256:

`0f174813e1e7195b2bb049e47b1f49486f405410a0555fe4f19d90cf68d89ae5`

Extract it from the repository root with:

    tar -xzf pn-forge-android-arm64-toolchain.tar.gz -C android/app/src/main/assets

The app copies this toolchain into its private storage on first launch.

## Build the Android app

After installing the required Android SDK and restoring the toolchain assets:

    gradle -p android assembleDebug

The APK is generated at:

    android/app/build/outputs/apk/debug/app-debug.apk

Install on a connected device:

    adb install -r android/app/build/outputs/apk/debug/app-debug.apk

## Upstream dependency

The project uses Pocket Native as a Git submodule. Its checked-out revision
is pinned by this repository.

Upstream: https://github.com/whovian64/pocket-native

## License

See the license terms of this project and its dependencies. Pocket Native
and LLVM/Clang retain their respective licenses.
