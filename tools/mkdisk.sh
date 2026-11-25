#!/usr/bin/env bash
# compila los programas de usuario (user/) y arma el disco albafs con disk/ + los ejecutables.
# uso: tools/mkdisk.sh [disco.img]   (por defecto target/alba-disk.img)
set -euo pipefail
cd "$(dirname "$0")/.."

OUT=${1:-target/alba-disk.img}
STAGE=target/disk
TARGET=aarch64-unknown-none-softfloat

# los programas se enlazan con su propio mapa de memoria (user/user.ld)
(cd user && RUSTFLAGS="-C link-arg=-Tuser.ld -C link-arg=-zmax-page-size=4096" \
  cargo build --release --target "$TARGET")

rm -rf "$STAGE"
mkdir -p "$STAGE"
cp disk/* "$STAGE"/
for src in user/src/bin/*.rs; do
  name=$(basename "$src" .rs)
  case "$name:${ALBA_DISK_TEST:-0}" in
    eco:0|fdtest:0|intruso:0|intruso2:0|lanzador:0|stress:0) continue ;;
  esac
  cp "user/target/$TARGET/release/$name" "$STAGE/$name"
  chmod +x "$STAGE/$name" # mkfs marca como ejecutables los archivos con permiso de ejecucion
done

python3 tools/mkfs.py "$STAGE" "$OUT"
