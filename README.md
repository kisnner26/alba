# alba

alba es un sistema operativo completamente nuevo, escrito desde cero por Kisnner.

no es una distribución de linux, no es un fork de ningún sistema existente y no se apoya en el código de ningún otro kernel. el kernel, los drivers, el sistema de archivos, la biblioteca estándar, el shell y la interfaz gráfica se escriben desde cero, empezando por la primera instrucción que ejecuta el procesador.

## objetivos

- **eficiencia**: nada corre si no hace falta. sin polling; cuando no hay trabajo el procesador duerme (`wfe`) y despierta por interrupción.
- **minimalismo**: kernel pequeño, pocas llamadas al sistema, cero dependencias externas dentro del kernel.
- **rapidez**: arranque en milisegundos y sin inicializar hardware que no se use.
- **optimización**: asignadores propios, estructuras sin bloqueos donde se pueda, copias evitadas, compilación con optimizaciones altas.
- **interfaz estilo macOS**: compositor propio con barra de menú, dock, ventanas con sombra y esquinas redondeadas, tipografía propia y animaciones fluidas.

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

**espacios de direcciones** ([src/vm.rs](src/vm.rs)): cada proceso tiene tablas propias (niveles 1, 2 y 3), un ASID de 8 bits y ve su imagen en `0x8000_0000` (solo lectura y ejecución), una página de datos en `0x8010_0000` y la pila bajo `0x8020_0000`. todo lo demás está sin mapear: ni siquiera existe la memoria de otros procesos ni la del kernel para EL0. el cargador copia la imagen de usuario embebida en el kernel a páginas nuevas y puestas a cero. el planificador cambia `TTBR0` solo cuando cambia el espacio. el test prueba que dos procesos que escriben en la misma dirección virtual no se pisan.

**modo usuario y llamadas al sistema** ([src/syscall.rs](src/syscall.rs), [src/user.rs](src/user.rs)):

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

- los programas de usuario siguen embebidos en la imagen del kernel; falta un cargador de ejecutables desde un sistema de archivos
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
7. drivers virtio: disco, red, teclado y ratón; sistema de archivos propio
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

para probar el manejador de excepciones, compilar con la feature `crash-test`, que ejecuta una instrucción `brk` al arrancar:

    cargo build --release --features crash-test

## estructura

    alba/
    ├── Cargo.toml
    ├── linker.ld          mapa de memoria del kernel
    ├── Makefile
    ├── tools/mkimage.py   genera la imagen plana a partir del ELF
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
        ├── vm.rs          espacios de direcciones y cargador
        ├── user.rs        programas de usuario de demostración
        ├── mmu.rs         memoria virtual
        ├── pmm.rs         memoria física
        └── heap.rs        asignador del kernel

## autor

Kisnner, creador de alba.

## licencia

MIT. ver [LICENSE](LICENSE).
