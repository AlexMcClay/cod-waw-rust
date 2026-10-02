cd /work/OAT/build
ZCG=$(find . -name ZoneCodeGenerator -type f | head -1)
mkdir -p /work/out2/T4
$ZCG --no-color -h /work/OAT/src/ZoneCode/Game/T4/T4_ZoneCode.h -c /work/OAT/src/ZoneCode/Game/T4/T4_Commands.txt -o /work/out2/T4 -g AssetStructTests -g ZoneMark
ls /work/out2/T4 | head
