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
| `0x4000_0000` - `0x4800_0000` | RAM (128 MiB) |
| `0x4008_0000` | inicio del kernel (`.text`, `.rodata`, `.data`, `.bss`) |
| tras `.bss` | pila de arranque de 64 KiB (`__stack_top`) |
| tras la pila | páginas libres administradas por `pmm` |

## flujo de arranque

1. QEMU carga el ELF en `0x4008_0000` y salta a `_start` en EL1.
2. `_start` (ensamblador): solo el core 0 continúa, los demás ejecutan `wfe`. Configura `sp`, limpia `.bss`, salta a `kmain`.
3. `kmain`: enciende la MMU, inicializa la memoria física, instala vectores, configura el GIC y el timer, crea las tareas, desenmascara interrupciones y queda como tarea idle (`wfe`).

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

- **MMU** ([src/mmu.rs](../src/mmu.rs)): mapeo identidad con bloques de 1 GiB. `0x0` dispositivos (Device-nGnRnE, sin ejecución), `0x4000_0000` RAM (Normal write-back, inner shareable). mantiene cachés encendidas; es provisional hasta tener tablas finas y W^X.
- **pmm** ([src/pmm.rs](../src/pmm.rs)): bitmap de páginas de 4 KiB. reserva rangos contiguos con búsqueda desde una pista (`HINT`).
- **heap** ([src/heap.rs](../src/heap.rs)): clases de 16 a 2048 bytes con listas libres intrusivas; cada bloque queda alineado a su tamaño. lo mayor a 2048 usa páginas contiguas. no es seguro desde irq.

## tareas y planificador

- una tarea = pila de 16 KiB + `sp` guardado. el cambio de contexto (`switch_to`) guarda `x19`-`x30` y `sp` en la pila saliente y carga los de la entrante.
- una tarea nueva arranca en `task_trampoline`, que activa interrupciones y llama a `task_main`.
- round-robin: el timer (10 Hz) levanta `NEED`; al final de `handle_irq`, después del EOI, `preempt_check` cambia de tarea dentro del propio manejador.
- `sleep` mueve la tarea a una lista de dormidas con un tick de despertar; `wake_sleepers` las devuelve a la cola cuando vencen.
- `exit` deja la tarea en una lista de zombis; su pila se libera desde otra tarea (`reap`), nunca desde la propia.
- invariante: toda mutación del planificador ocurre con irq enmascaradas (un solo core). `irq::save` / `irq::restore` son las secciones críticas.

## decisiones

- **rust sin dependencias**: el objetivo es que todo sea código propio; `core` y `alloc` vienen con el compilador.
- **aarch64 primero**: misma familia que el hardware de desarrollo y sin el legado de x86.
- **QEMU `virt`**: hardware simple y documentado para avanzar rápido; el hardware real llega en la etapa 11.
- **round-robin simple**: suficiente para validar el cambio de contexto; prioridades vendrán con la carga real.

## pruebas

`tests/boot.sh` compila, arranca alba en QEMU unos segundos y verifica las líneas esperadas y la ausencia de excepciones. corre en cada push en GitHub Actions.
