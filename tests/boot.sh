#!/usr/bin/env bash
# arranca alba en qemu un rato y comprueba que aparezcan las lineas esperadas
# y que no haya excepciones ni panics. uso: tests/boot.sh  (BOOT_SECS=12 por defecto)
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release
KERNEL=target/aarch64-unknown-none-softfloat/release/alba
OUT=$(mktemp)

qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 128M -nographic \
  -kernel "$KERNEL" >"$OUT" 2>&1 &
PID=$!
sleep "${BOOT_SECS:-12}"
kill "$PID" 2>/dev/null || true
wait "$PID" 2>/dev/null || true

expected=(
  "alba 0.0.1"
  "memoria:"
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
)

fail=0
for e in "${expected[@]}"; do
  grep -qF -- "$e" "$OUT" || { echo "falta: $e"; fail=1; }
done
if grep -qE "excepcion|panic|ERROR" "$OUT"; then
  echo "la salida contiene una excepcion, panic o ERROR"
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "--- salida ---"
  cat "$OUT"
  exit 1
fi
echo "ok: arranque verificado"
