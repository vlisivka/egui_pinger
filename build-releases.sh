#!/bin/bash
set -ueE -o pipefail

NAME=$(<Cargo.toml grep '^name =' | cut -d '"' -f 2)
VERSION=$(<Cargo.toml grep '^version =' | cut -d '"' -f 2)
RELEASES_DIR="./release"

panic() {
  echo "ERROR: $*"
  exit 1
}

[ -x ~/.cargo/bin/cross ] || cargo install cross --git https://github.com/cross-rs/cross \
  || panic "Cannot install cross compiler. Install it manually, see https://github.com/cross-rs/cross ."

[ -x ~/.cargo/bin/cargo-xwin ] || cargo install cargo-xwin \
  || panic "Cannot install cargo-xwin. Install it manually: cargo install cargo-xwin"

which clang >/dev/null 2>&1 || panic "clang is not installed. Install it: sudo dnf install clang lld"
which llvm-lib >/dev/null 2>&1 || panic "llvm-lib is not installed. Install it: sudo dnf install llvm"

cargo clean || panic "Cannot clean \"target\" directory."
rm -rf "$RELEASES_DIR/*" || panic "Cannot clean \"$RELEASES_DIR\" directory."
mkdir -p "$RELEASES_DIR" || panic "Cannot create \"$RELEASES_DIR\" directory."

ARCHES=(
  x86_64-unknown-linux-gnu
#  aarch64-unknown-linux-gnu
#  x86_64-pc-windows-gnu
#  x86_64-pc-windows-msvc
)

for TARGET in "${ARCHES[@]}"
do
  echo "--- Building for $TARGET ---"
  if [[ "$TARGET" == *"-msvc" ]]; then
    cargo xwin build --release --target "$TARGET" || panic "Cannot build release for \"$TARGET\" target using cargo-xwin."
  else
    cross build --release --target "$TARGET" || panic "Cannot build release for \"$TARGET\" target using cross."
  fi
  TARGET_BUILD_DIR="./target/$TARGET/release/"

  if [ -x "$TARGET_BUILD_DIR/$NAME" ]
  then
    EXECUTABLE="$NAME"
    ARCHIVE="$RELEASES_DIR/$TARGET-$VERSION.tar.gz"

    echo "--- Packaging $ARCHIVE ---"
    TEMP_DIR=$(mktemp -d)
    mkdir -p "$TEMP_DIR/bin" "$TEMP_DIR/share/applications" "$TEMP_DIR/share/icons/hicolor/scalable/apps"

    cp "$TARGET_BUILD_DIR/$EXECUTABLE" "$TEMP_DIR/bin/"
    cp "assets/linux/egui_pinger.desktop" "$TEMP_DIR/share/applications/"
    cp "assets/linux/egui_pinger.svg" "$TEMP_DIR/share/icons/hicolor/scalable/apps/"

    rm -f "$ARCHIVE"
    tar -zcf "$ARCHIVE" -C "$TEMP_DIR" bin share || panic "Cannot make archive \"$ARCHIVE\" from \"$TEMP_DIR\"."

    rm -rf "$TEMP_DIR"

  elif [ -x "$TARGET_BUILD_DIR/$NAME.exe" ]
  then
    EXECUTABLE="$NAME.exe"
    ARCHIVE=$(readlink -f "$RELEASES_DIR/$TARGET-$VERSION.zip")

    rm -f "$ARCHIVE"
    ( cd "$TARGET_BUILD_DIR" && zip "$ARCHIVE" "$EXECUTABLE" ) || panic "Cannot make archive \"$ARCHIVE\" using file \"$EXECUTABLE\" from \"$TARGET_BUILD_DIR\" ."

  else
    panic "Cannot find executable for \"$TARGET\" target."
  fi

done
