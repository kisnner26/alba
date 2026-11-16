# albafs

sistema de archivos propio de alba. versión 1: simple a propósito.

- sectores de 512 bytes, todos los enteros little endian
- directorio plano (sin carpetas), archivos contiguos en disco

## disposición

| sector | contenido |
|--------|-----------|
| 0 | superbloque |
| 1 .. 1+D-1 | directorio (D sectores) |
| 1+D .. | datos de los archivos, uno tras otro, alineados a sector |

## superbloque (sector 0)

| offset | tamaño | campo |
|--------|--------|-------|
| 0 | 8 | magia `ALBAFS01` |
| 8 | 4 | versión (1) |
| 12 | 4 | número de archivos |
| 16 | 4 | sector del directorio (1) |
| 20 | 4 | sectores del directorio |
| 24 | 4 | sectores totales del disco |

## entrada de directorio (64 bytes, 8 por sector)

| offset | tamaño | campo |
|--------|--------|-------|
| 0 | 40 | nombre, terminado en cero |
| 40 | 4 | sector de inicio |
| 44 | 4 | tamaño en bytes |
| 48 | 4 | banderas (bit 0 = ejecutable) |
| 52 | 12 | reservado |

## crear un disco

    python3 tools/mkfs.py disk/ target/alba-disk.img

lee los archivos de `disk/` (los que tengan permiso de ejecución quedan marcados como ejecutables) y deja un disco de al menos 1 MiB.

## límites conocidos

- sin carpetas, sin permisos más allá de la bandera de ejecución
- archivos contiguos: crear o crecer un archivo obliga a reescribir el disco; la escritura desde el kernel llegará con una versión 2 con asignación por extensiones
