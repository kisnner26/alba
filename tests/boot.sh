#!/usr/bin/env bash
# arranca alba en qemu y comprueba la salida. siete pruebas:
#   1. arranque normal: lineas esperadas y ninguna excepcion, panic, ERROR o FALLO
#   2. feature wx-test: escribir en el texto del kernel debe dar un fallo de datos (W^X activo)
#   3. -m 256M: el kernel debe leer el tamano real de la RAM del DTB
#   4. shell: sesion tecleada por el UART (ls, cat, echo, leer con retroceso, uptime, exit)
#   5. energia: sin interrupciones de timer en reposo (kernel sin tics)
#   6. rendimiento: lanzar programas seguidos es casi instantaneo y apenas despierta al timer
#   7. cache: el segundo cat no vuelve a pedir datos al disco
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

# run_qemu <segundos> [memoria, 128M por defecto] [entrada] [entrada2] -> deja la salida en $OUT
# la entrada (formato printf %b) se envia por el UART 3 segundos despues de arrancar; la segunda,
# si existe, 6 segundos despues de la primera
run_qemu() {
  OUT=$(mktemp)
  ./tools/mkdisk.sh "$DISK" >/dev/null
  DISK_BEFORE=$(python3 -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest())' "$DISK")
  local qemu=(qemu-system-aarch64 -machine virt -cpu cortex-a72 -m "${2:-128M}" -nographic
    -global virtio-mmio.force-legacy=false
    -drive file="$DISK",if=none,format=raw,id=hd0 -device virtio-blk-device,drive=hd0
    -kernel "$KERNEL")
  if [ -n "${3:-}" ]; then
    ( sleep 3; printf '%b' "$3"
      if [ -n "${4:-}" ]; then sleep 6; printf '%b' "$4"; fi
      sleep 20 ) | "${qemu[@]}" >"$OUT" 2>&1 &
  else
    "${qemu[@]}" >"$OUT" 2>&1 &
  fi
  local pid=$!
  sleep "$1"
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  # la salida serie termina las lineas con \r\n: se normaliza para poder comparar lineas enteras
  tr -d '\r' <"$OUT" >"$OUT.lf" && mv "$OUT.lf" "$OUT"
}

fail=0

# --- 1. arranque normal ---
build --features autotest
run_qemu "${BOOT_SECS:-12}"

expected=(
  "alba 0.0.1"
  "memoria:"
  "virtio: ranura"
  "(bloque) version 2"
  "disco: 2048 sectores de 512 bytes"
  "disco: sector 0 = \"ALBAFS01\""
  "disco: escritura y lectura del sector 2047 ok"
  "albafs: 14 archivos"
  "LEEME.txt (82 bytes)"
  "hola.txt: hola desde el disco de alba"
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
  "exec hola: pid"
  "[eco] argc=3"
  "[eco] argv[1]=uno"
  "[eco] argv[2]=dos tres"
  "termino con codigo 7"
  "esperar un pid inexistente da error"
  "exec basura: rechazado (no es un ELF)"
  "exec hola.txt: rechazado (no es ejecutable)"
  "exec noexiste: rechazado (no existe)"
)

for e in "${expected[@]}"; do
  grep -qF -- "$e" "$OUT" || { echo "falta: $e"; fail=1; }
done
if [ "$(grep -cE 'disco concurrente: lecturas correctas, esperas=[1-9][0-9]*' "$OUT")" -ne 2 ]; then
  echo "fallo la prueba de lecturas concurrentes del disco"; cat "$OUT"; exit 1
fi
if grep -qE "excepcion|panic|ERROR|FALLO" "$OUT"; then
  echo "la salida contiene una excepcion, panic, ERROR o FALLO"
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

# --- 4. shell y consola: entrada por interrupcion del UART, eco, retroceso y programas del disco ---
# arranque normal (sin autotest): va directo a la shell. se teclea una sesion completa;
# en "holx" + retroceso + "a alba" el programa leer debe recibir "hola alba"
build
run_qemu 10 128M 'ls\ncat hola.txt\necho uno dos\ndesconocido\nleer\nholx\x7fa alba\nuptime\nexit\n'
disk_after=$(python3 -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest())' "$DISK")
if [ "$DISK_BEFORE" != "$disk_after" ]; then
  echo "el arranque normal modifico el disco"; exit 1
fi
sh_expected=(
  "alba shell. escribe help para ver los comandos."
  "init: shell lanzada"
  "[leer] escribe algo:"
  "[leer] recibi: hola alba"
  "sh: no encontrado: desconocido"
  "arriba desde hace"
)
for e in "${sh_expected[@]}"; do
  grep -qF -- "$e" "$OUT" || { echo "falta en la sesion de shell: $e"; cat "$OUT"; exit 1; }
done
# salidas que ocupan una linea entera: cat, echo y ls. como la sesion se teclea de golpe, el
# prompt "alba> " puede quedar delante de la primera salida: se admite como prefijo
for e in "hola desde el disco de alba" "uno dos"; do
  grep -qE "^(alba> )?$e$" "$OUT" || { echo "falta linea exacta: $e"; cat "$OUT"; exit 1; }
done
grep -qE "^hola.txt +28 bytes" "$OUT" || { echo "falta la linea de ls para hola.txt"; cat "$OUT"; exit 1; }
echo "ok: shell verificada"

# --- 5. energia: en reposo el timer no debe despertar al procesador ---
# sin tics periodicos: 6 s de reposo entre dos `uptime` no pueden costar interrupciones de timer.
# un kernel con tics a 10 Hz habria sumado unas 60; aqui solo cuenta lanzar el segundo comando.
run_qemu 14 128M 'uptime\n' 'uptime\n'
irqs=($(grep -oE "interrupciones de timer: [0-9]+" "$OUT" | grep -oE "[0-9]+$"))
if [ "${#irqs[@]}" -lt 2 ]; then
  echo "no se obtuvieron dos mediciones de uptime"; cat "$OUT"; exit 1
fi
idle=$((irqs[1] - irqs[0]))
if [ "$idle" -gt 4 ]; then
  echo "el timer desperto ${idle} veces en 6 s de reposo (maximo 4)"; cat "$OUT"; exit 1
fi
echo "ok: reposo sin tics verificado (${idle} interrupciones de timer en 6 s)"

# --- 6. rendimiento: lanzar programas debe ser casi instantaneo ---
# seis `uptime` seguidos (cada uno: leer el ELF del disco, cargarlo, ejecutarlo y esperarlo) no
# pueden consumir mas de 500 ms de reloj ni mas de 3 interrupciones de timer
run_qemu 7 128M 'uptime\nuptime\nuptime\nuptime\nuptime\nuptime\n'
t=($(grep -oE "desde hace [0-9]+\.[0-9]" "$OUT" | grep -oE "[0-9]+\.[0-9]$" | tr -d '.'))
q=($(grep -oE "interrupciones de timer: [0-9]+" "$OUT" | grep -oE "[0-9]+$"))
if [ "${#t[@]}" -lt 6 ]; then echo "faltan mediciones de uptime"; cat "$OUT"; exit 1; fi
span=$((t[5] - t[0]))      # en decimas de segundo
wake=$((q[5] - q[0]))
if [ "$span" -gt 5 ] || [ "$wake" -gt 3 ]; then
  echo "lanzar 5 programas tardo ${span}/10 s y causo ${wake} interrupciones de timer (maximo 0,5 s y 3)"
  cat "$OUT"; exit 1
fi
echo "ok: rendimiento verificado (5 programas lanzados en ${span}/10 s, ${wake} interrupciones de timer)"

# --- 7. cache: el segundo cat no debe enviar peticiones a virtio ---
run_qemu 7 128M 'uptime\ncat hola.txt\nuptime\ncat hola.txt\nuptime\n'
requests=($(grep -oE "peticiones de disco: [0-9]+" "$OUT" | grep -oE "[0-9]+$"))
if [ "${#requests[@]}" -ne 3 ] ||
   [ "$(grep -cE '^(alba> )?hola desde el disco de alba$' "$OUT")" -ne 2 ] ||
   [ "${requests[1]}" -le "${requests[0]}" ] ||
   [ "${requests[2]}" -ne "${requests[1]}" ]; then
  echo "se esperaba una lectura inicial y cero peticiones para el segundo cat"; cat "$OUT"; exit 1
fi
echo "ok: cache verificada (peticiones de disco: ${requests[*]})"

# deja el binario normal compilado
build >/dev/null 2>&1
