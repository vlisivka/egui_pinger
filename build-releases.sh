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

#cargo clean || panic "Cannot clean \"target\" directory."
rm -rf "$RELEASES_DIR/*" || panic "Cannot clean \"$RELEASES_DIR\" directory."
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
    EXECUTABLE="$NAME"
    ARCHIVE="$RELEASES_DIR/$TARGET-$VERSION.tar.gz"

    rm -f "$ARCHIVE"
    tar -zcf "$ARCHIVE" -C "$TARGET_BUILD_DIR" "$EXECUTABLE" || panic "Cannot make archive \"$ARCHIVE\" using file \"$EXECUTABLE\" from \"$TARGET_BUILD_DIR\"."

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
