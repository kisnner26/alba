# arquitectura de alba

este documento describe cómo está construido alba hoy y por qué. se actualiza con cada etapa.

## plataforma

- cpu: aarch64 (armv8-a), nivel de excepción EL1 para el kernel
- máquina de referencia: `qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 128M`
- lenguaje: rust sin biblioteca estándar (`no_std`) más ensamblador en línea; sin dependencias externas

## mapa de memoria

| rango | contenido |
|-------|-----------|
| `0x0800_0000` | distribuidor del GICv2 |
| `0x0801_0000` | interfaz de cpu del GICv2 |
| `0x0900_0000` | UART PL011 |
| `0x4000_0000` - `0x4100_0000` | zona del kernel (16 MiB), solo EL1 |
| `0x4008_0000` | inicio del kernel (`.text`, `.rodata`, `.data`, `.bss`) |
| tras `.bss` | pila de arranque de 64 KiB (`__stack_top`); después, páginas del kernel administradas por `pmm` |
| `0x4100_0000` | sección `.user`: código y datos de los programas de usuario (EL0 lectura, escritura, ejecución) |
| tras `.user` - fin de RAM (del DTB, hasta 1 GiB) | zona de usuario: pilas y datos, EL0 lectura y escritura, sin ejecución |

## flujo de arranque

1. QEMU carga la imagen plana (`alba.bin`, generada por `tools/mkimage.py`) en `0x4008_0000`, reconoce la cabecera arm64 (`ARM\x64` en el offset 56) y salta a `_start` en EL1 con la dirección del DTB en `x0`.
2. `_start` (ensamblador): solo el core 0 continúa, los demás ejecutan `wfe`. Configura `sp`, limpia `.bss`, salta a `kmain`.
3. `kmain(dtb)`: lee la RAM del DTB (`fdt.rs`), enciende la MMU, inicializa la memoria física, instala vectores, configura el GIC y el timer, crea las tareas, desenmascara interrupciones y queda como tarea idle (`wfe`).

## device tree

- el DTB lo carga el firmware (QEMU lo deja en RAM, dentro de la zona de usuario). `fdt::ram` recorre el bloque de estructura y devuelve base y tamaño del nodo `memory`. solo hace lecturas alineadas de 32 bits y de bytes, así que funciona con la MMU apagada.
- `pmm::reserve` marca las páginas del DTB como ocupadas para que no se entreguen a un proceso.
- si no hay DTB válido se asumen 128 MiB.

## excepciones e interrupciones

- tabla de 16 vectores alineada a 2 KiB. solo se usan sync (`0x200`) e irq (`0x280`) de EL1 con `SP_EL1`; el resto cae en `handle_bad`.
- cada entrada guarda un marco de 272 bytes en la pila:

| offset | contenido |
|--------|-----------|
| 0 - 240 | `x0` - `x30` |
| 248 | `elr_el1` |
| 256 | `spsr_el1` |
| 264 | `sp_el0` |

- `elr_el1`, `spsr_el1` y `sp_el0` son registros globales del procesador pero pertenecen a cada tarea; por eso viajan en el marco. omitirlos rompía la apropiación (una tarea retomaba en la dirección de otra).
- GICv2: el timer fisico es la PPI 30. el EOI se hace antes de cualquier cambio de tarea.

## memoria

- **MMU** ([src/mmu.rs](../src/mmu.rs)): mapeo identidad. nivel 1 con un bloque de 1 GiB para dispositivos (Device-nGnRnE, sin ejecución, sin acceso EL0) y una tabla de nivel 2 con bloques de 2 MiB para la RAM (Normal write-back). las caches están encendidas.
  - el procesador prohíbe ejecutar en EL1 una página escribible desde EL0, por eso el kernel (16 MiB, `AP=00`) y el código de usuario (`AP=01`, `PXN`) están en bloques distintos. el primer intento de dar acceso EL0 a toda la RAM colgó el arranque por esa regla.
  - el primer bloque de 2 MiB (donde vive la imagen) usa una tabla de nivel 3 con páginas de 4 KiB: texto `AP_RO` ejecutable, rodata `AP_RO` + `PXN`, el resto (datos, bss, pilas, memoria libre) escribible + `PXN`. los demás bloques del kernel también son `PXN`. el linker alinea cada sección a página y exporta `__text_*` y `__rodata_*`.
  - pendiente: un espacio de direcciones por proceso.
- **pmm** ([src/pmm.rs](../src/pmm.rs)): bitmap de páginas de 4 KiB con dos zonas (kernel y usuario). reserva rangos contiguos con una pista de búsqueda por zona.
- **heap** ([src/heap.rs](../src/heap.rs)): clases de 16 a 2048 bytes con listas libres intrusivas; cada bloque queda alineado a su tamaño. lo mayor a 2048 usa páginas contiguas. no es seguro desde irq.

## tareas y planificador

- una tarea = pila de 16 KiB + `sp` guardado. el cambio de contexto (`switch_to`) guarda `x19`-`x30` y `sp` en la pila saliente y carga los de la entrante.
- una tarea nueva arranca en `task_trampoline`, que activa interrupciones y llama a `task_main`.
- round-robin: el timer (10 Hz) levanta `NEED`; al final de `handle_irq`, después del EOI, `preempt_check` cambia de tarea dentro del propio manejador.
- `sleep` mueve la tarea a una lista de dormidas con un tick de despertar; `wake_sleepers` las devuelve a la cola cuando vencen.
- `exit` deja la tarea en una lista de zombis; su pila se libera desde otra tarea (`reap`), nunca desde la propia.
- invariante: toda mutación del planificador ocurre con irq enmascaradas (un solo core). `irq::save` / `irq::restore` son las secciones críticas.

## modo usuario y llamadas al sistema

- una tarea de usuario tiene pila de kernel (16 KiB, zona kernel) y pila de usuario (8 KiB, zona usuario). arranca en `task_user_trampoline`, que limpia los registros y hace `eret` a EL0 con `SPSR=0` (interrupciones activas).
- las excepciones desde EL0 entran por los vectores `0x400` (sync) y `0x480` (irq). el marco guarda `sp_el0`.
- `svc #0`: número en `x8`, argumentos `x0`-`x2`, resultado en `x0` (`u64::MAX` = error). tabla en [src/syscall.rs](../src/syscall.rs).
- el kernel nunca confía en un puntero de EL0: `write` comprueba que el rango esté dentro de la zona de usuario.
- cualquier otra excepción desde EL0 (por ejemplo leer memoria del kernel) termina solo esa tarea.
- los programas de demostración viven en la sección `.user` ([src/user.rs](../src/user.rs)) y no pueden llamar a código del kernel salvo por `svc`.

## decisiones

- **rust sin dependencias**: el objetivo es que todo sea código propio; `core` y `alloc` vienen con el compilador.
- **aarch64 primero**: misma familia que el hardware de desarrollo y sin el legado de x86.
- **QEMU `virt`**: hardware simple y documentado para avanzar rápido; el hardware real llega en la etapa 11.
- **dos zonas de memoria fijas**: la forma más corta de tener aislamiento real kernel/usuario antes de las tablas finas.
- **round-robin simple**: suficiente para validar el cambio de contexto; prioridades vendrán con la carga real.

## pruebas

`tests/boot.sh` hace tres pruebas en QEMU: (1) arranque normal con las líneas esperadas, tareas, modo usuario, intruso detenido y ninguna excepción; (2) con la feature `wx-test`, escribir en el texto del kernel debe dar un fallo de datos (clase `0x25`); (3) con `-m 256M` el kernel debe leer 256 MiB del DTB. corre en cada push en GitHub Actions.
