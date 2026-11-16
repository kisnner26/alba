#!/usr/bin/env bash
# arranca alba en qemu y comprueba la salida. tres pruebas:
#   1. arranque normal: lineas esperadas y ninguna excepcion, panic o ERROR
#   2. feature wx-test: escribir en el texto del kernel debe dar un fallo de datos (W^X activo)
#   3. -m 256M: el kernel debe leer el tamano real de la RAM del DTB
# uso: tests/boot.sh   (BOOT_SECS=12 por defecto para la prueba 1)
set -euo pipefail
cd "$(dirname "$0")/.."

ELF=target/aarch64-unknown-none-softfloat/release/alba
KERNEL=$ELF.bin
DISK=target/alba-disk.img

# compila y genera la imagen plana con cabecera arm64 (QEMU entrega el DTB en x0)
build() {
  cargo build --release "$@"
  python3 tools/mkimage.py "$ELF" "$KERNEL"
}

# run_qemu <segundos> [memoria, 128M por defecto] -> deja la salida en $OUT
run_qemu() {
  OUT=$(mktemp)
  python3 tools/mkfs.py disk "$DISK"
  qemu-system-aarch64 -machine virt -cpu cortex-a72 -m "${2:-128M}" -nographic \
    -global virtio-mmio.force-legacy=false \
    -drive file="$DISK",if=none,format=raw,id=hd0 -device virtio-blk-device,drive=hd0 \
    -kernel "$KERNEL" >"$OUT" 2>&1 &
  local pid=$!
  sleep "$1"
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
}

fail=0

# --- 1. arranque normal ---
build
run_qemu "${BOOT_SECS:-12}"

expected=(
  "alba 0.0.1"
  "memoria:"
  "virtio: ranura"
  "(bloque) version 2"
  "heap: suma=49995000"
  "[a] fin"
  "[b] fin"
  "[ocupada-1]"
  "[ocupada-2]"
  "[usuario pid="
  "] iteracion 3"
  "] fin"
  "[intruso] intentando leer memoria del kernel"
  "fallo en modo usuario: tarea"
  "far=0x40080000"
  "] datos propios intactos"
  "[intruso2] intentando escribir en su propio codigo"
  "far=0x80000000"
)

for e in "${expected[@]}"; do
  grep -qF -- "$e" "$OUT" || { echo "falta: $e"; fail=1; }
done
if grep -qE "excepcion|panic|ERROR" "$OUT"; then
  echo "la salida contiene una excepcion, panic o ERROR"
  fail=1
fi
if [ "$fail" -ne 0 ]; then
  echo "--- salida del arranque normal ---"
  cat "$OUT"
  exit 1
fi
echo "ok: arranque verificado"

# --- 2. W^X: escribir en el texto debe fallar con un fallo de datos (clase 0x25) ---
build --features wx-test
run_qemu 3
if ! grep -q "excepcion sincrona: clase=0x25" "$OUT"; then
  echo "W^X no se aplico: escribir en el texto del kernel no fallo"
  echo "--- salida wx-test ---"
  cat "$OUT"
  build >/dev/null 2>&1
  exit 1
fi
echo "ok: W^X verificado"

# --- 3. RAM dinamica: con 256 MiB el kernel debe verla completa, leida del DTB ---
build
run_qemu 3 256M
for e in "tamano=256 MiB" "de 262144 KiB"; do
  grep -qF -- "$e" "$OUT" || { echo "falta con 256M: $e"; cat "$OUT"; exit 1; }
done
echo "ok: RAM dinamica verificada"

# deja el binario normal compilado
build >/dev/null 2>&1
