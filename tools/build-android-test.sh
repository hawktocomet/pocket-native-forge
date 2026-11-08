#!/usr/bin/env bash

GEN="$HOME/source/pocket-recompiler/target/yoshi-generation"
NDK="$HOME/Android/Sdk/ndk/27.2.12479018"
BIN="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
CC="$BIN/aarch64-linux-android30-clang"
STRIP="$BIN/llvm-strip"

OUT="$HOME/source/pocket-recompiler/target/yoshi-android-test"
OBJ="$OUT/objects"
LOGDIR="$OUT/logs"
SO="$OUT/00040000001A4100.so"

JOBS=4

mkdir -p "$OBJ" "$LOGDIR"

if [ ! -x "$CC" ]; then
    echo "ERROR: Android compiler missing: $CC"
    exit 1
fi

echo "========== ANDROID PARALLEL BUILD =========="
echo "Compiler: $CC"
echo "Parallel jobs: $JOBS"
echo "Input: $GEN"
echo "Output: $OUT"
echo

cat "$NDK/source.properties"

echo
echo "========== COMPILING GENERATED C =========="

TOTAL=$(find "$GEN" -maxdepth 1 -type f -name '*.c' | wc -l)
echo "Total source files: $TOTAL"
echo "Existing reusable objects: $(find "$OBJ" -maxdepth 1 -name '*.o' -type f | wc -l)"
echo

find "$GEN" -maxdepth 1 -type f -name '*.c' -print0 |
xargs -0 -r -P "$JOBS" -I{} bash -c '
    SRC="$1"
    GEN="$2"
    OBJ="$3"
    CC="$4"
    LOGDIR="$5"

    NAME="$(basename "${SRC%.c}")"
    OBJECT="$OBJ/$NAME.o"
    LOG="$LOGDIR/$NAME.log"

    if [ -s "$OBJECT" ] && [ "$OBJECT" -nt "$SRC" ]; then
        echo "REUSED: $NAME"
        exit 0
    fi

    "$CC" -c "$SRC" \
        -o "$OBJECT" \
        -I"$GEN" \
        -O2 \
        -ffp-contract=off \
        -fno-math-errno \
        -w \
        -fPIC \
        -fvisibility=hidden \
        -fno-stack-protector \
        > "$LOG" 2>&1

    RESULT=$?

    if [ "$RESULT" -ne 0 ]; then
        echo "FAILED: $NAME (exit $RESULT)"
        exit 1
    fi

    echo "COMPILED: $NAME"
' _ '{}' "$GEN" "$OBJ" "$CC" "$LOGDIR"

COMPILED_OBJECTS=$(find "$OBJ" -maxdepth 1 -type f -name '*.o' | wc -l)
FAILED_LOGS=$(grep -l -E 'error:|fatal error:' "$LOGDIR"/*.log 2>/dev/null | wc -l)

echo
echo "========== COMPILATION SUMMARY =========="
echo "Expected C files: $TOTAL"
echo "Available objects: $COMPILED_OBJECTS"
echo "Error logs: $FAILED_LOGS"

if [ "$COMPILED_OBJECTS" -ne "$TOTAL" ]; then
    echo "COMPILATION INCOMPLETE"
    echo "Check logs in: $LOGDIR"
    exit 1
fi

echo
echo "========== LINKING ANDROID ARM64 LIBRARY =========="

"$CC" -shared -nostdlib \
    -Wl,-z,max-page-size=16384 \
    -o "$SO" \
    "$OBJ"/*.o

RESULT=$?

if [ "$RESULT" -ne 0 ]; then
    echo "LINK FAILED: $RESULT"
    exit 1
fi

"$STRIP" --strip-unneeded "$SO"

echo
echo "========== LIBRARY RESULT =========="
file "$SO"
ls -lh "$SO"

echo
echo "========== ELF VALIDATION =========="
"$BIN/llvm-readelf" -h "$SO" | grep -E 'Class:|Machine:|Type:'
"$BIN/llvm-readelf" -d "$SO" | grep -E 'NEEDED|SONAME' || true

echo
echo "BUILD SUCCEEDED: $SO"
