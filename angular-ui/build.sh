#!/bin/sh
#

rm -rf dist
ng build -c production
#rm -rf ../static/*
cp -r dist/angular-ui/*.* ../static
