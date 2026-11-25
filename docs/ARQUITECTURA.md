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
| `0x4100_0000` - fin de RAM (del DTB, hasta 1 GiB) | memoria de procesos: el kernel la ve por identidad (sin ejecución), EL0 no |
| `0x8000_0000` - `0x801f_c000` | (solo dentro de cada proceso) segmentos del programa, con los permisos de cada uno |
| `0x801f_c000` | (solo dentro de cada proceso) página de guarda, sin mapear |
| `0x801f_e000` | (solo dentro de cada proceso) pila de 2 páginas |
| `0x8020_0000` | (solo dentro de cada proceso) tope de la pila de usuario |

## flujo de arranque

1. QEMU carga la imagen plana (`alba.bin`, generada por `tools/mkimage.py`) en `0x4008_0000`, reconoce la cabecera arm64 (`ARM\x64` en el offset 56) y salta a `_start` en EL1 con la dirección del DTB en `x0`.
2. `_start` (ensamblador): solo el core 0 continúa, los demás ejecutan `wfe`. Configura `sp`, limpia `.bss`, salta a `kmain`.
3. `kmain(dtb)`: lee la RAM del DTB (`fdt.rs`), enciende la MMU, inicializa la memoria física, instala vectores, configura el GIC y el timer, crea las tareas, desenmascara interrupciones y queda como tarea idle (`wfe`).

## device tree

- el DTB lo carga el firmware (QEMU lo deja en RAM, dentro de la zona de usuario). `fdt::ram` recorre el bloque de estructura y devuelve base y tamaño del nodo `memory`. solo hace lecturas alineadas de 32 bits y de bytes, así que funciona con la MMU apagada.
- `pmm::reserve` marca las páginas del DTB como ocupadas para que no se entreguen a un proceso.
- si no hay DTB válido se asumen 128 MiB.

## energia

la eficiencia energetica es la prioridad maxima y se traduce en reglas concretas del kernel:

- **sin tics.** el tiempo se lee del contador del sistema (`CNTPCT`), que cuenta sin gastar interrupciones. el comparador (`CNTP_CVAL`) solo se programa para el proximo evento: el vencimiento del primer `sleep` o el final del tick actual si hay otra tarea lista. sin eventos, el temporizador se apaga. `sched::reprogram` recalcula esto en cada cambio de estado.
- **idle de verdad.** la tarea 0 es la idle: no entra en la cola de turnos y solo se elige cuando no queda nada mas. ejecuta `wfi`. antes participaba del round-robin y gastaba un turno de 100 ms durmiendo mientras otro programa esperaba.
- **sin sondeo.** consola y disco usan interrupciones; `Wait::Input` y `Wait::Disk` bloquean la tarea y despiertan desde el manejador. solo el arranque, antes de habilitar interrupciones, espera por sondeo.
- **menos peticiones.** `blk` agrupa hasta 64 KiB por peticion. la [cache de disco](../src/cache.rs) retiene 128 sectores de 512 bytes en 16 conjuntos de 8, con reemplazo lru por conjunto para acotar el trabajo de busqueda. si falta un sector, se pide el rango completo para conservar el agrupamiento. las escrituras invalidan el rango antes de enviarlo. una lectura solo evita el acceso a virtio mientras todos sus sectores sigan en cache.
- **medicion.** `uptime` muestra las interrupciones de timer (syscall 11) y las peticiones que llegaron a virtio-blk (syscall 12). `tests/boot.sh` comprueba el reposo, el tiempo de lanzamiento y que dos ejecuciones seguidas de `cat hola.txt` solo necesiten peticiones en la primera.
- **limites conocidos:** solo hay un core activo (los demas hacen `wfe` en lugar de apagarse con PSCI); no hay escalado de frecuencia (no existe en QEMU); las cifras son de emulacion, no de consumo real.

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

- **MMU** ([src/mmu.rs](../src/mmu.rs)): mapeo identidad para el kernel. nivel 1 con un bloque de 1 GiB para dispositivos (Device-nGnRnE, sin ejecución, sin acceso EL0) y una tabla de nivel 2 para la RAM (Normal write-back). las caches están encendidas.
  - el primer bloque de 2 MiB (donde vive la imagen) usa una tabla de nivel 3 con páginas de 4 KiB: texto `AP_RO` ejecutable, rodata `AP_RO` + `PXN`, el resto (datos, bss, pilas, memoria libre) escribible + `PXN`. los demás bloques también son `PXN` y ninguno es accesible desde EL0. el linker alinea cada sección a página y exporta `__text_*` y `__rodata_*`.
  - esta es la tabla de las tareas del kernel (ASID 0). los procesos de usuario usan la suya (ver más abajo).
- **pmm** ([src/pmm.rs](../src/pmm.rs)): bitmap de páginas de 4 KiB con dos zonas (kernel y usuario). reserva rangos contiguos con una pista de búsqueda por zona.
- **heap** ([src/heap.rs](../src/heap.rs)): clases de 16 a 2048 bytes con listas libres intrusivas; cada bloque queda alineado a su tamaño. lo mayor a 2048 usa páginas contiguas. no es seguro desde irq.

## tareas y planificador

- una tarea = pila de 16 KiB + `sp` guardado. el cambio de contexto (`switch_to`) guarda `x19`-`x30` y `sp` en la pila saliente y carga los de la entrante.
- una tarea nueva arranca en `task_trampoline`, que activa interrupciones y llama a `task_main`.
- round-robin: el timer (10 Hz) levanta `NEED`; al final de `handle_irq`, después del EOI, `preempt_check` cambia de tarea dentro del propio manejador.
- `sleep` mueve la tarea a una lista de dormidas con un tick de despertar; `wake_sleepers` las devuelve a la cola cuando vencen.
- `exit` deja la tarea en una lista de zombis; su pila se libera desde otra tarea (`reap`), nunca desde la propia.
- invariante: toda mutación del planificador ocurre con irq enmascaradas (un solo core). `irq::save` / `irq::restore` son las secciones críticas.

## disco y sistema de archivos

- **virtio-mmio** ([src/virtio.rs](../src/virtio.rs)): QEMU `virt` expone 32 ranuras de 0x200 bytes desde `0x0a00_0000` (en el bloque de dispositivos, sin caché). se exige la versión moderna (`-global virtio-mmio.force-legacy=false`).
- **virtio-blk** ([src/blk.rs](../src/blk.rs)): inicialización según la especificación (reinicio, `ACKNOWLEDGE`, `DRIVER`, característica `VERSION_1`, `FEATURES_OK`, cola 0, `DRIVER_OK`). una cola de 8 entradas y una petición a la vez, con una cadena de 3 descriptores (cabecera, hasta 64 KiB de datos, estado). durante el arranque espera por sondeo con un límite; después, la tarea espera la interrupción y las demás esperan su turno sin sobrescribir la petición. la memoria de la cola sale de `pmm` (identidad). pendiente: mantenimiento de caché para hardware real.
- **albafs** ([src/fs.rs](../src/fs.rs), [albafs.md](albafs.md)): montaje con validación del superbloque y del directorio (tamaños, límites, que ningún archivo se salga del disco), lectura de archivos sector a sector. solo lectura por ahora.

## modo usuario y llamadas al sistema

- un proceso de usuario tiene su propio espacio de direcciones ([src/vm.rs](../src/vm.rs)): tablas de nivel 1, 2 y 3 y un ASID (1-255; el 0 es del kernel). las entradas 0 (dispositivos) y 1 (RAM) del nivel 1 se comparten con el kernel, pero sus páginas son solo EL1; la entrada 2 apunta a las tablas del proceso.
- el cargador ([src/vm.rs](../src/vm.rs), [src/elf.rs](../src/elf.rs)) lee el ELF desde albafs, valida cabecera y segmentos, y por cada `PT_LOAD` reserva páginas puestas a cero, copia `filesz` bytes (el resto hasta `memsz`, el `.bss`, queda en ceros) y mapea con los permisos del segmento: texto R-X, rodata R--, datos RW- sin ejecución. rechaza segmentos W+X, solapados, sin alinear a página, fuera de `0x8000_0000..0x801f_c000` o un punto de entrada que no caiga en código. después mapea 2 páginas de pila RW sin ejecución. las entradas son `nG` (etiquetadas con el ASID), así que cambiar de proceso no vacía la TLB.
- `mmu::switch_ttbr` reescribe `TTBR0` solo si cambia el espacio. al terminar el proceso, `destroy` invalida su ASID en la TLB y libera tablas y páginas.
- una tarea de usuario arranca en `task_user_trampoline`, que limpia los registros y hace `eret` a EL0 con `SPSR=0` (interrupciones activas).
- las excepciones desde EL0 entran por los vectores `0x400` (sync) y `0x480` (irq). el marco guarda `sp_el0`.
- `svc #0`: número en `x8`, argumentos `x0`-`x2`, resultado en `x0` (`u64::MAX` = error). tabla en [src/syscall.rs](../src/syscall.rs).
- el kernel nunca confía en un puntero de EL0: `write` comprueba que el rango caiga entero dentro de una región mapeada del proceso actual.
- cualquier otra excepción desde EL0 (por ejemplo leer memoria del kernel) termina solo esa tarea, con código de salida 139 (128 + 11, como una violación de segmento): la shell lo informa y un script no lo confunde con un éxito.
- los programas son un crate aparte ([user/](../user/)): la biblioteca `alba` da las llamadas al sistema y `println!`, y `user/user.ld` les da un mapa de memoria con segmentos alineados a página. `tools/mkdisk.sh` los compila y los empaqueta en el disco; el kernel ya no lleva programas embebidos. como se enlazan en `0x8000_0000`, el mismo binario corre en cualquier proceso.

## decisiones

- **rust sin dependencias**: el objetivo es que todo sea código propio; `core` y `alloc` vienen con el compilador.
- **aarch64 primero**: misma familia que el hardware de desarrollo y sin el legado de x86.
- **QEMU `virt`**: hardware simple y documentado para avanzar rápido; el hardware real llega en la etapa 11.
- **dos zonas de memoria**: el kernel conserva un mapeo identidad solo-EL1 de toda la RAM para acceder a cualquier página sin tablas adicionales; los procesos solo ven lo que sus tablas mapean.
- **round-robin simple**: suficiente para validar el cambio de contexto; prioridades vendrán con la carga real.

## pruebas

`tests/boot.sh` genera un disco albafs y hace quince pruebas en QEMU: (1) arranque normal con las líneas esperadas (incluida la detección del disco virtio, una escritura y lectura de sector y el listado y lectura de albafs), tareas, modo usuario, intruso detenido, escritura en código de solo lectura detenida, dos procesos que escriben en la misma dirección virtual sin pisarse y ninguna excepción; (2) con la feature `wx-test`, escribir en el texto del kernel debe dar un fallo de datos (clase `0x25`); (3) con `-m 256M` el kernel debe leer 256 MiB del DTB; (4) una sesion de shell tecleada por el UART (ls, cat, echo, programas con argumentos, leer con retroceso, errores, exit); (5) energia: 6 s de reposo no deben costar interrupciones de timer; (6) rendimiento: lanzar programas seguidos casi no consume tiempo ni despierta al timer; (7) cache: el segundo `cat hola.txt` no envia peticiones al disco; (8) persistencia: los cambios sobreviven a un reinicio con el mismo disco; (9) presupuestos de recursos que solo pueden bajar; (10) virtio-gpu: píxeles del scanout, teclado y ratón inyectados por QMP; (11) mil procesos sin fugas de páginas ni de ASID; (12) lectores de ELF y albafs mutados sin pánico; (13) pantalla de fallo fatal; (14) **corte de energía**: `fscorte` escribe sin parar, QEMU muere con SIGKILL en un instante distinto cinco veces y tras cada corte el disco monta y cada archivo está ausente, vacío o completo; (15) **tuberías**: una copia de unos 13 KiB por una tubería de 512 bytes debe ser idéntica (`cmp`), un lector que se cierra antes de tiempo no deja colgado al escritor y un segundo comando inexistente no deja colgado al primero. corre en cada push en GitHub Actions.

límite de la prueba 14: el disco virtual conserva lo que QEMU ya recibió del invitado, así que prueba que el directorio se publica de forma atómica entre peticiones, no que sobreviva a perder escrituras ya confirmadas por un corte real de hardware (eso necesita un dispositivo que simule pérdida de caché).
