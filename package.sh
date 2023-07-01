#!/bin/sh

BASE_NAME=portforwarder
if [ "X$1" = "X" ]; then
   echo "Version required"
   exit
fi
git pull
sh build.sh
VERSION=$1
echo Building version $VERSION
DIR=target/x86_64-unknown-linux-musl/release
cd $DIR
TARGET=/tmp/$BASE_NAME-linux-$VERSION.tar.gz
rm -f $TARGET
tar zcvf $TARGET $BASE_NAME
cd -
