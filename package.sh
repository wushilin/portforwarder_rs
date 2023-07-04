#!/bin/sh

export IFS=";"
BINARIES="portforwarder;test1"
BASE_NAME=portforwarder
ARCH=x86_64-unknown-linux-musl
echo Updating git...
git pull
sh build.sh
VERSION=`cat Cargo.toml | grep version | head -1 | sed 's/^.*=\|"\|\\s//g'`
echo Building version $VERSION
DIR=target/$ARCH/release
TARGET=$(pwd -P)/target/$BASE_NAME-$ARCH-$VERSION.tar
TARGET_Z=$TARGET.gz
echo Target: $TARGET_Z
cd $DIR
rm -f $TARGET
rm -f $TARGET_Z
for next in $BINARIES
do
  if [ ! -f $next ]; then
    continue
  fi
  echo "Adding file $next"
  if [ -f $TARGET ]; then
    tar --append --file $TARGET $next
  else
    tar cf $TARGET $next
  fi
done
gzip $TARGET
cd - >> /dev/null
