# alba

alba es un sistema operativo completamente nuevo, escrito desde cero por Kisnner.

no es una distribución de linux, no es un fork de ningún sistema existente y no se apoya en el código de ningún otro kernel. el kernel, los drivers, el sistema de archivos, la biblioteca estándar, el shell y la interfaz gráfica se escriben desde cero, empezando por la primera instrucción que ejecuta el procesador.

## objetivos

**prioridad máxima: eficiencia energética.** alba apunta a procesadores ARM de bajo consumo y a una autonomía de batería excepcional. cada decisión se evalúa primero por cuánta energía cuesta, y el rendimiento sale de la misma regla: el trabajo más rápido es el que no se hace, y el procesador más barato es el que duerme.

- **el procesador duerme siempre que puede.** el kernel no tiene tics periódicos: el temporizador solo se programa para el próximo evento real (un `sleep` que vence, el turno de otra tarea lista) y se apaga cuando no hay ninguno. la tarea inactiva ejecuta `wfi` y no gasta turnos. en reposo, cero despertares.
- **nada de sondeo.** el disco y la consola avisan por interrupción; las tareas que esperan se bloquean y el procesador duerme mientras tanto.
- **menos trabajo por tarea.** peticiones de disco de varios sectores (un programa se lee de una sola vez), cambio de espacio de direcciones sin vaciar la TLB (ASID), cachés encendidas desde el arranque.
- **medido, no supuesto.** `uptime` muestra las interrupciones de timer y las peticiones enviadas al disco y los tests fallan si el kernel despierta al procesador en reposo o si lanzar programas se vuelve lento. ver "energía y rendimiento".
- **rendimiento.** arranque en milisegundos, programas que se lanzan sin que se note, asignadores propios y copias evitadas. el objetivo declarado es superar a macOS en eficiencia por vatio; hoy solo se mide en emulación, así que todavía no hay comparación honesta (ver más abajo).
- **minimalismo.** kernel pequeño, pocas llamadas al sistema, cero dependencias externas dentro del kernel. menos código es menos que ejecutar y menos que despertar.
- **interfaz estilo macOS.** compositor propio con barra de menú, dock, ventanas con sombra y esquinas redondeadas, tipografía propia y animaciones fluidas, sin pagar energía por ello: solo se redibuja lo que cambia y la pantalla estática no despierta a nadie.

## energía y rendimiento

medido en QEMU (`make test` lo comprueba en cada cambio):

| métrica | resultado |
|---------|-----------|
| interrupciones de timer en 6 s de reposo | 0 |
| lanzar 5 programas seguidos (leer ELF, cargar, ejecutar, esperar) | menos de 100 ms en total, 0 interrupciones de timer |
| espera de entrada de consola y de disco | por interrupción, sin sondeo |
| repetir `cat hola.txt` con la caché caliente | 0 peticiones adicionales al disco |

lo que **no** está medido todavía, para no engañar: consumo real en vatios. QEMU emula el procesador, no su consumo, así que estas cifras miden despertares y latencia, además de peticiones de disco. el consumo requiere mediciones de potencia. la comparación con macOS solo será honesta con mediciones en hardware real (etapa 11), con el mismo procesador y la misma carga.

pendiente para bajar más el consumo, en orden de impacto:

1. apagar los núcleos secundarios y despertarlos solo bajo demanda (PSCI `CPU_OFF` / `CPU_ON`), y estados de reposo profundo (`CPU_SUSPEND`)
2. escalado de frecuencia y voltaje en hardware real, con política de "correr rápido y dormir" (race to idle)
3. agrupar temporizadores cercanos para despertar una vez en lugar de varias
4. pantalla: redibujar solo las regiones que cambian y apagar la GPU cuando nada se mueve
5. herramienta de contabilidad de despertares por proceso

## recursos

línea base medida con `tools/medir.sh` en qemu 11.1.3, máquina `virt`,
cortex-a72, tcg con `-icount shift=0,sleep=on`, 128 mib y consola serie.

| métrica | medida |
|---------|--------|
| imagen plana del kernel | 61.664 bytes |
| disco mínimo de la imagen actual | 229.888 bytes |
| ram reservada u ocupada al llegar a la shell | 1.978.368 bytes |
| páginas propias de la shell | 7 de kernel y 6 de usuario |
| arranque hasta la entrada a la shell | 105.348 µs |
| lanzar y esperar `echo`, primera ejecución | 45 µs |
| lanzar y esperar `echo`, segunda ejecución | 37 µs |
| interrupciones de temporizador en un segundo de reposo | 0 |

el arranque y el lanzamiento se miden con el contador físico del huésped
bajo el reloj emulado de qemu. la ram incluye reservas de arranque, kernel y
procesos vivos, y se cuenta en páginas físicas de 4096 bytes. la tabla por
programa y los límites están en [tests/presupuestos.txt](tests/presupuestos.txt).
la prueba completa falla al rebasar un límite. los límites existentes solo
bajan si una mejora medida lo permite. estas cifras no miden vatios ni
comparan a alba con otro sistema operativo.

## qué significa "desde cero"

lo que se escribe desde cero, sin copiar ni portar código de otros proyectos:

- arranque, vectores de excepciones, controlador de interrupciones, temporizador
- gestor de memoria física, memoria virtual y asignador del kernel
- procesos, planificador, llamadas al sistema
- drivers (UART, virtio: disco, red, GPU, teclado y ratón)
- sistema de archivos
- biblioteca estándar de C o equivalente, shell y programas de usuario
- servidor gráfico, compositor, widgets y tipografía
- bootloader propio

lo que se usa solo como herramienta de desarrollo, no como parte del sistema:

- `rustc` y `cargo` para compilar, y `qemu` para ejecutar sin hardware real

la última etapa de la hoja de ruta es reemplazar también esas herramientas por las propias, para que el sistema se pueda construir a sí mismo.

## estado actual

versión 0.0.1. corre en aarch64 sobre la máquina `virt` de QEMU.

| etapa | contenido | estado |
|-------|-----------|--------|
| 1 | arranque en ensamblador, linker script, UART PL011, handler de pánico | listo |
| 2 | vectores de excepciones EL1, GICv2, temporizador genérico a 10 Hz | listo |
| 3 | MMU con cachés, gestor de páginas físicas, heap del kernel | listo |
| 4 | tareas del kernel, planificador round-robin con apropiación, sleep, exit | listo |
| 5 | modo usuario (EL0), llamadas al sistema, aislamiento de memoria kernel/usuario | listo |
| 6 | memoria fina: W^X con páginas de 4 KiB, RAM leída del DTB, espacios de direcciones por proceso y cargador de imagen | listo |
| 7 | virtio-blk, sistema de archivos propio (albafs), ejecutables ELF, consola y shell | en curso: disco por interrupciones, albafs de lectura, `exec`/`wait` con argumentos, consola y shell listos |
| 8 | energía: kernel sin tics, idle con `wfi`, sin sondeo | en curso: sin tics y sin sondeo listos; faltan los puntos de la lista de pendientes |

salida actual al arrancar:

    alba 0.0.1
    hola desde mi propio sistema operativo
    memoria: 130468 KiB libres de 131072 KiB
    heap: suma=49995000 box=42 big=1048576 usado=1184 KiB
    heap: tras liberar, usado=32 KiB
    [a] paso 1
    [b] paso 1
    [ocupada] sigo corriendo, ticks=7
    [a] paso 2
    ...

### detalles de lo que existe

**arranque** ([src/main.rs](src/main.rs), [linker.ld](linker.ld)): el kernel se carga en `0x40080000`. solo el core 0 continúa; los demás se duermen. se configura la pila, se limpia `.bss` y se salta a Rust.

**excepciones e interrupciones** ([src/trap.rs](src/trap.rs), [src/gic.rs](src/gic.rs), [src/timer.rs](src/timer.rs)): tabla de vectores propia, guardado completo de registros, diagnóstico de excepciones síncronas (clase, `esr`, `elr`, `far`), controlador GICv2 y temporizador del sistema.

**memoria** ([src/mmu.rs](src/mmu.rs), [src/pmm.rs](src/pmm.rs), [src/heap.rs](src/heap.rs)):

- MMU encendida con mapeo identidad (bloques de 1 GiB y 2 MiB, y páginas de 4 KiB en el primer bloque): dispositivos sin caché ni acceso desde EL0; 16 MiB de RAM solo para el kernel; el resto es memoria de procesos, que EL0 solo ve a través de su propio espacio de direcciones
- W^X en el kernel: texto de solo lectura y ejecutable, rodata de solo lectura, datos/pilas/heap escribibles sin ejecución; el arranque de prueba comprueba que escribir en el texto da un fallo
- páginas físicas de 4 KiB con bitmap, con reserva de rangos contiguos
- heap por clases de tamaño de 16 a 2048 bytes con listas libres; páginas contiguas para tamaños mayores

**arranque estándar** ([src/fdt.rs](src/fdt.rs), [tools/mkimage.py](tools/mkimage.py)): alba empieza con la cabecera de imagen arm64 (el protocolo de arranque de linux). QEMU y los firmwares reales la reconocen y entregan el device tree en `x0`; el kernel lo recorre con un lector propio para saber cuánta RAM hay y reserva el propio DTB. la imagen plana la genera un script propio a partir del ELF, sin `objcopy`.

**disco y sistema de archivos** ([src/virtio.rs](src/virtio.rs), [src/blk.rs](src/blk.rs), [src/fs.rs](src/fs.rs), [docs/albafs.md](docs/albafs.md)): transporte virtio-mmio moderno con sondeo de las 32 ranuras, driver de bloque con una cola de tres descriptores por petición, y `albafs`, un formato propio (superbloque, directorio plano, archivos contiguos). `tools/mkfs.py` crea el disco a partir de la carpeta `disk/`. el kernel valida el superbloque y el directorio antes de confiar en ellos. durante el arranque se espera por sondeo; después, las tareas esperan la finalización por interrupción y comparten la cola por turnos.

**espacios de direcciones** ([src/vm.rs](src/vm.rs)): cada proceso tiene tablas propias (niveles 1, 2 y 3), un ASID de 8 bits y ve los segmentos de su programa desde `0x8000_0000`, cada uno con sus permisos, y la pila bajo `0x8020_0000`. todo lo demás está sin mapear: ni siquiera existe la memoria de otros procesos ni la del kernel para EL0. el planificador cambia `TTBR0` solo cuando cambia el espacio. el test prueba que dos procesos que escriben en la misma dirección virtual no se pisan.

**ejecutables** ([src/elf.rs](src/elf.rs), [src/vm.rs](src/vm.rs), [user/](user/)): los programas de usuario son un crate aparte con su propia biblioteca mínima (llamadas al sistema, `println!`, pánico), el embrión de la biblioteca estándar de alba. se compilan a ELF con su propio mapa de memoria (`user/user.ld`), se empaquetan en el disco con `tools/mkdisk.sh` y `exec` los lee de albafs. el cargador ELF valida cada campo del archivo, copia cada segmento a páginas nuevas y puestas a cero (el `.bss` queda en ceros), le da los permisos del segmento y rechaza cualquier segmento escribible y ejecutable a la vez, segmentos solapados, fuera de rango o un punto de entrada fuera del código. los tests comprueban que un archivo basura, uno no ejecutable y uno inexistente se rechazan sin afectar al sistema.

**modo usuario y llamadas al sistema** ([src/syscall.rs](src/syscall.rs)):

- las tareas de usuario corren en EL0 con su propia pila de usuario y pila de kernel
- `svc #0` con el número en `x8` y argumentos en `x0`-`x2`; llamadas actuales: `write`, `exit`, `sleep`, `getpid`, `yield`
- el kernel valida cada puntero que llega de EL0 antes de usarlo
- un programa que toca memoria del kernel o de dispositivos recibe un fallo de hardware y su tarea se termina sin afectar al sistema; el arranque de prueba lo verifica con un programa intruso

**procesos y planificador** ([src/sched.rs](src/sched.rs), [src/irq.rs](src/irq.rs)):

- tareas con pila propia de 16 KiB y cambio de contexto en ensamblador que guarda solo `x19`-`x30` y `sp`
- round-robin con rodajas de 100 ms; el timer marca la necesidad de cambiar y el cambio ocurre al final del manejador de irq, después del EOI
- una tarea que nunca cede la cpu es apropiada igualmente
- `sleep` saca la tarea de la cola: las tareas dormidas no consumen cpu y el core duerme con `wfe` si no hay trabajo
- `exit` libera la pila de la tarea desde otro contexto
- la tarea 0 (arranque) hace de idle

limitaciones conocidas, a resolver en las próximas etapas:

- los programas no reciben argumentos ni variables de entorno; `exec` es una función del kernel, todavía no una llamada al sistema (no hay `fork`/`exec` desde usuario)
- la RAM máxima soportada es 1 GiB (una tabla de nivel 2)
- el heap no es seguro de usar desde manejadores de interrupción
- solo un core activo

## hoja de ruta

1. arranque y UART (listo)
2. excepciones, GIC y temporizador (listo)
3. memoria física, MMU y heap (listo)
4. tareas, planificador y cambio de contexto (listo)
5. modo usuario (EL0) y llamadas al sistema (listo)
6. memoria fina: W^X, lectura del DTB, espacio de direcciones por proceso y cargador (listo)
7. drivers virtio: disco (listo), red, teclado y ratón; sistema de archivos propio (albafs de lectura, listo; escritura y carga de ejecutables, pendiente)
8. shell, biblioteca estándar propia y programas de usuario
9. GPU virtio, framebuffer, compositor y escritorio estilo macOS
10. multinúcleo (SMP)
11. bootloader propio y hardware real (raspberry pi)
12. compilador y herramientas propias para que alba se construya a sí mismo

## compilar y ejecutar

requisitos en macOS (apple silicon):

    brew install qemu rustup
    export PATH=/opt/homebrew/opt/rustup/bin:$PATH
    rustup default stable

ejecutar:

    make run          # compila, genera la imagen plana y la arranca en QEMU
    make test         # pruebas de arranque, modo usuario, W^X y RAM

para salir de QEMU: `ctrl+a`, luego `x`.

en la consola, `ps` muestra estado, tiempo en cpu, bloqueos y despertares por
tarea. el tiempo usa el contador del procesador entre cambios de tarea,
sin programar interrupciones adicionales.

en la consola, `edit archivo` edita texto utf-8 de hasta 4096 bytes.
`:ver` muestra el contenido, `:borrar` lo vacía en memoria, `:guardar`
publica el cambio en albafs v2 y `:salir` cierra el editor. `mv` renombra
hacia un destino libre. `files` muestra el directorio y `top` toma una
instantánea de los procesos.

### salida gráfica en qemu

alba carga el escritorio si qemu presenta `virtio-gpu-device`. la consola
serie sigue disponible. el escritorio dibuja desde el espacio de usuario
sobre un framebuffer propio y refresca solo la región que indica. la prueba
gráfica comprueba la captura y un segundo de reposo sin interrupciones del
temporizador. con `virtio-keyboard-device` y `virtio-tablet-device` el
escritorio recibe el teclado y el puntero sin sondeo. `f2` cambia entre
inglés y español latinoamericano. se pueden seleccionar las vistas del dock
y arrastrar el panel por su cabecera.

### probar en windows con qemu

en la máquina de desarrollo, `make windows` genera `target/alba-windows-qemu.zip` con el kernel, el disco de prueba y un lanzador. en windows, instala [qemu de 64 bits](https://www.qemu.org/download/#windows), descomprime el zip y abre `iniciar-alba.cmd`. en la consola puedes usar `help`, `ls`, `cat hola.txt` y `uptime`; para salir, pulsa `ctrl+a` y luego `x`.

el lanzador busca `qemu-system-aarch64.exe` en el `path` y en la carpeta de instalación habitual `%ProgramFiles%\qemu`. para guardar la salida de alba, abre una terminal en la carpeta extraída y ejecuta este comando.

    .\iniciar-alba.cmd --log

`alba.log` se guarda junto al lanzador y se reemplaza en cada ejecución con `--log`. si qemu falla antes del arranque, su mensaje de error aparece en la terminal.

alba usa la uart desde la primera línea del arranque. el lanzador conecta esa uart explícitamente a la terminal y desactiva el vídeo. así puede registrar la salida sin interrumpir la entrada por teclado ni necesitar una ventana gráfica de respaldo. [opciones de consola de qemu](https://www.qemu.org/docs/master/system/invocation.html).

`./tests/package.sh` genera el zip, comprueba los finales de línea de windows y prueba el arranque, `ls` y el registro con los archivos extraídos.

en un equipo x86-64, qemu emula el procesador arm64 por software, sin aceleración de hardware. el lanzador de este paquete abre la consola serie y monta el disco en modo de solo lectura.

para probar el manejador de excepciones, compilar con la feature `crash-test`, que ejecuta una instrucción `brk` al arrancar:

    cargo build --release --features crash-test

## estructura

    alba/
    ├── Cargo.toml
    ├── linker.ld          mapa de memoria del kernel
    ├── Makefile
    ├── tools/mkimage.py   genera la imagen plana a partir del ELF
    ├── tools/mkfs.py      crea un disco albafs desde la carpeta disk/
    ├── user/              programas de usuario (crate propio: biblioteca + programas)
    ├── disk/              archivos que van al disco de prueba
    ├── build.rs           reenlaza si cambia linker.ld
    ├── tests/boot.sh      pruebas de arranque, modo usuario y W^X
    ├── docs/ARQUITECTURA.md
    └── src/
        ├── main.rs        arranque, UART, kmain
        ├── fdt.rs         lector de device tree
        ├── trap.rs        vectores y manejadores de excepciones
        ├── gic.rs         controlador de interrupciones
        ├── timer.rs       temporizador del sistema
        ├── irq.rs         secciones críticas
        ├── sched.rs       tareas y planificador
        ├── syscall.rs     llamadas al sistema
        ├── virtio.rs      transporte virtio-mmio
        ├── blk.rs         driver de bloque
        ├── fs.rs          albafs (lectura)
        ├── vm.rs          espacios de direcciones y cargador
        ├── elf.rs         lector de ELF64
        ├── mmu.rs         memoria virtual
        ├── pmm.rs         memoria física
        └── heap.rs        asignador del kernel

## autor

Kisnner, creador de alba.

## licencia

MIT. ver [LICENSE](LICENSE).
