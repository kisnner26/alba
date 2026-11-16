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

    /* imagen de los programas de usuario: se carga aqui (LMA) y el cargador (vm.rs) la copia
       a paginas propias de cada proceso, donde se ve en 0x80000000 (VMA) */
    . = ALIGN(4096);
    __user_load = .;
    .user 0x80000000 : AT(__user_load) {
        KEEP(*(.user.text*))
        KEEP(*(.user.rodata*))
    }
    __user_size = SIZEOF(.user);
    __user_load_end = __user_load + __user_size;
    __image_size = __user_load_end - __text_start;

    /DISCARD/ : { *(.comment) *(.eh_frame*) }
}
