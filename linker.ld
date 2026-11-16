ENTRY(_start)

SECTIONS
{
    . = 0x40080000;

    .text : {
        KEEP(*(.text.boot))
        *(.text*)
    }

    .rodata : ALIGN(8) { *(.rodata*) }
    .data   : ALIGN(8) { *(.data*) }

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
