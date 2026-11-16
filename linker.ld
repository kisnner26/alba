ENTRY(_start)

/* cada seccion del kernel empieza y termina en limite de pagina (4 KiB):
   mmu.rs asigna permisos distintos a cada una (texto RX, rodata R, datos RW) */
SECTIONS
{
    . = 0x40080000;

    __text_start = .;
    .text : {
        KEEP(*(.text.boot))
        *(.text*)
    }
    . = ALIGN(4096);
    __text_end = .;

    __rodata_start = .;
    .rodata : { *(.rodata*) }
    . = ALIGN(4096);
    __rodata_end = .;

    __data_start = .;
    .data : { *(.data*) }

    .bss : ALIGN(8) {
        __bss_start = .;
        *(.bss*)
        *(COMMON)
        . = ALIGN(8);
        __bss_end = .;
    }

    . = ALIGN(16);
    . += 0x10000;
    __stack_top = .;

    /* codigo y datos de los programas de usuario: bloque propio accesible desde EL0 */
    . = 0x41000000;
    .user : {
        KEEP(*(.user.text*))
        KEEP(*(.user.rodata*))
    }
    . = ALIGN(4096);
    __user_end = .;

    /DISCARD/ : { *(.comment) *(.eh_frame*) }
}
