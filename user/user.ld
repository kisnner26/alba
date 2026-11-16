/* mapa de memoria de un programa de usuario de alba.
   cada segmento empieza en limite de pagina y tiene permisos propios (W^X):
   texto R-X, rodata R--, datos y bss RW-. el kernel mapea cada uno asi. */
ENTRY(_start)

PHDRS
{
    text   PT_LOAD FLAGS(5);
    rodata PT_LOAD FLAGS(4);
    data   PT_LOAD FLAGS(6);
}

SECTIONS
{
    . = 0x80000000;

    .text : ALIGN(4096) {
        KEEP(*(.text._start))
        *(.text*)
    } :text

    .rodata : ALIGN(4096) { *(.rodata*) } :rodata

    .data : ALIGN(4096) { *(.data*) } :data
    .bss : ALIGN(4096) {
        *(.bss*)
        *(COMMON)
    } :data

    /DISCARD/ : { *(.comment) *(.eh_frame*) *(.note*) }
}
