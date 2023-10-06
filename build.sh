#!/bin/sh

echo Building UI
cd angular-ui
sh build.sh
cd -
echo "UI Done"
echo "Building binary"
cargo build --release
echo "Done"
