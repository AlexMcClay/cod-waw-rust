#!/bin/bash
set -x
apt-get update -qq && apt-get install -y -qq wget git make >/dev/null
cd /work
[ -d OAT ] || git clone --depth 1 https://github.com/Laupetin/OpenAssetTools.git OAT
cd OAT
git submodule update --init --recursive --depth 1
PREMAKE_NO_PROMPT=1 ./generate.sh
cd build
ls
make -j8 ZoneCodeGenerator config=release_x64 2>&1 | tail -30
find . -name ZoneCodeGenerator -type f
ZCG=$(find . -name ZoneCodeGenerator -type f | head -1)
mkdir -p /work/out/T4
$ZCG --no-color -h /work/OAT/src/ZoneCode/Game/T4/T4_ZoneCode.h -c /work/OAT/src/ZoneCode/Game/T4/T4_Commands.txt -o /work/out/T4 -g ZoneLoad
ls -R /work/out | head -100
