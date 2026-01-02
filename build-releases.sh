#!/bin/bash
set -ue

NAME=$(<Cargo.toml grep '^name =' | cut -d '"' -f 2)
VERSION=$(<Cargo.toml grep '^version =' | cut -d '"' -f 2)
RELEASES_DIR="./release"

panic() {
  echo "ERROR: $*"
  exit 1
}

[ -x ~/.cargo/bin/cross ] || cargo install cross --git https://github.com/cross-rs/cross \
  || panic "Cannot install cross compiler. Install it manually, see https://github.com/cross-rs/cross ."

cargo clean || panic "Cannot clean \"target\" directory."
rm -rf "$RELEASES_DIR/*"
mkdir -p "$RELEASES_DIR" || panic "Cannot create \"$RELEASES_DIR\" directory."

ARCHES=(
  x86_64-unknown-linux-gnu
  aarch64-unknown-linux-gnu
  x86_64-pc-windows-gnu
  i686-pc-windows-gnu
)

for TARGET in "${ARCHES[@]}"
do
  cross build --release --target "$TARGET" || panic "Cannot build release for \"$TARGET\" target."

  TARGET_BUILD_DIR="./target/$TARGET/release/"

  if [ -x "$TARGET_BUILD_DIR/$NAME" ]
  then
    EXECUTABLE="$TARGET_BUILD_DIR/$NAME"
    ARCHIVE="$RELEASES_DIR/$TARGET-$VERSION.tar.gz"

    rm -f "$ARCHIVE"
    tar -zcf "$ARCHIVE" "$EXECUTABLE" || panic "Cannot make archive \"$ARCHIVE\" using file \"$EXECUTABLE\"."

  elif [ -x "$TARGET_BUILD_DIR/$NAME.exe" ]
  then
    EXECUTABLE="$TARGET_BUILD_DIR/$NAME.exe"
    ARCHIVE="$RELEASES_DIR/$TARGET-$VERSION.zip"

    rm -f "$ARCHIVE"
    zip "$ARCHIVE" "$EXECUTABLE" || panic "Cannot make archive \"$ARCHIVE\" using file \"$EXECUTABLE\"."

  else
    panic "Cannot find executable for \"$TARGET\" target."
  fi

done
