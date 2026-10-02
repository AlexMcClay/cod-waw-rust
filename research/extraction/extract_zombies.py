#!/usr/bin/env python3
"""
Extract Zombies-mode content from a local Call of Duty: World at War install.

  python extract_zombies.py "<WaW install dir>" "<output dir>"

- Decompresses zone/english/nazi_zombie_*.ff (IWffu100, v387, zlib) and dumps
  every inline RawFile asset (gsc/csc/csv/menu/str/cfg/...) by scanning for the
  RawFile header pattern {name=-1, len, buffer=-1} followed by name\0 data\0.
- Copies zombie-related entries (sounds, images, weapons, etc.) out of the
  main/*.iwd archives (plain zip files).
- Writes a manifest.json per source.

For personal use with your own copy of the game.
"""
import json, os, re, struct, sys, zipfile, zlib

ZOMBIE_FF = ["nazi_zombie_prototype", "nazi_zombie_asylum", "nazi_zombie_sumpf",
             "nazi_zombie_factory"]
SUFFIXES = ["", "_patch", "_load"]
RAW_EXT = re.compile(rb"^[\w\-/\\\.]+\.(gsc|csc|csv|menu|str|cfg|txt|vision|gdt|atr|shock|rmb|xpo|arena|def)$", re.I)
IWD_KEYWORDS = ("zombie", "zmb", "nazi_z", "prototype", "perk", "mysterybox",
                "treasure_chest", "powerup", "insta_kill", "double_points",
                "max_ammo", "nuke", "carpenter", "ray_gun", "raygun", "zom_",
                "zm_", "/zombie", "wunderwaffe", "monkey", "teleporter")


def decompress_ff(path):
    data = open(path, "rb").read()
    if data[:8] not in (b"IWffu100", b"IWff0100"):
        raise ValueError(f"{path}: unknown magic {data[:8]!r}")
    version = struct.unpack_from("<I", data, 8)[0]
    body = data[12:]
    if data[:8] == b"IWffu100":
        d = zlib.decompressobj()
        body = d.decompress(body) + d.flush()
    return version, body


def scan_rawfiles(zone):
    out = []
    marker = b"\xff\xff\xff\xff"
    i = 0
    n = len(zone)
    while True:
        i = zone.find(marker, i)
        if i < 0 or i + 13 > n:
            break
        length = struct.unpack_from("<I", zone, i + 4)[0]
        if zone[i + 8:i + 12] == marker and 0 < length < 8_000_000:
            name_end = zone.find(b"\x00", i + 12, i + 12 + 256)
            if name_end > i + 12:
                name = zone[i + 12:name_end]
                if RAW_EXT.match(name):
                    start = name_end + 1
                    blob = zone[start:start + length]
                    if len(blob) == length:
                        out.append((name.decode("latin-1"), blob))
                        i = start + length
                        continue
        i += 1
    return out


def safe_join(root, rel):
    rel = rel.replace("\\", "/").lstrip("/")
    p = os.path.normpath(os.path.join(root, rel))
    if not p.startswith(os.path.normpath(root)):
        raise ValueError("path escape: " + rel)
    return p


def extract_ff(install, out_root):
    zone_dir = os.path.join(install, "zone", "english")
    summary = {}
    for base in ZOMBIE_FF:
        for suf in SUFFIXES:
            name = base + suf
            path = os.path.join(zone_dir, name + ".ff")
            if not os.path.exists(path):
                continue
            version, zone = decompress_ff(path)
            dest = os.path.join(out_root, "fastfiles", name)
            os.makedirs(dest, exist_ok=True)
            raws = scan_rawfiles(zone)
            files = []
            for rname, blob in raws:
                p = safe_join(os.path.join(dest, "raw"), rname)
                os.makedirs(os.path.dirname(p), exist_ok=True)
                with open(p, "wb") as f:
                    f.write(blob)
                files.append({"name": rname, "size": len(blob)})
            # every printable asset-ish name in the zone, useful for later parsing
            names = sorted(set(m.decode() for m in re.findall(
                rb"[a-z0-9_]{3,}(?:/[a-z0-9_\.]+)*", zone) if len(m) < 96 and (b"/" in m or b"_" in m)))
            with open(os.path.join(dest, "asset_names.txt"), "w") as f:
                f.write("\n".join(names))
            manifest = {"source": name + ".ff", "version": version,
                        "zone_bytes": len(zone), "rawfiles": files}
            with open(os.path.join(dest, "manifest.json"), "w") as f:
                json.dump(manifest, f, indent=1)
            summary[name] = len(files)
            print(f"[ff] {name}: v{version}, {len(zone):,} bytes, {len(files)} rawfiles")
    return summary


def extract_iwd(install, out_root):
    main = os.path.join(install, "main")
    dest = os.path.join(out_root, "iwd")
    count = 0
    index = []
    for fn in sorted(os.listdir(main)):
        if not fn.lower().endswith(".iwd"):
            continue
        with zipfile.ZipFile(os.path.join(main, fn)) as z:
            for info in z.infolist():
                low = info.filename.lower()
                if info.is_dir() or not any(k in low for k in IWD_KEYWORDS):
                    continue
                p = safe_join(dest, info.filename)
                os.makedirs(os.path.dirname(p), exist_ok=True)
                with z.open(info) as src, open(p, "wb") as dst:
                    while chunk := src.read(1 << 20):
                        dst.write(chunk)
                index.append({"iwd": fn, "path": info.filename, "size": info.file_size})
                count += 1
        print(f"[iwd] {fn}: running total {count}")
    with open(os.path.join(dest, "manifest.json"), "w") as f:
        json.dump(index, f, indent=1)
    return count


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    install, out = sys.argv[1], sys.argv[2]
    what = sys.argv[3] if len(sys.argv) > 3 else "all"
    os.makedirs(out, exist_ok=True)
    if what in ("all", "ff"):
        extract_ff(install, out)
    if what in ("all", "iwd"):
        print("iwd files:", extract_iwd(install, out))
