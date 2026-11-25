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

## versión 2, especificación

la versión 1 se sigue montando en modo de solo lectura. v2 conserva sectores
de 512 bytes, enteros little endian y un directorio plano de 64 archivos como
máximo. el disco v2 contiene dos copias de los metadatos en posiciones fijas.

| sectores | contenido |
|----------|-----------|
| 0 y 9 | cabeceras de las copias a y b, respectivamente |
| 1–8 y 10–17 | directorios de a y b, respectivamente |
| 18 .. total - 2 | datos |
| total - 1 | reservado para las pruebas del dispositivo |

### cabecera de cada copia

| offset | tamaño | campo |
|--------|--------|-------|
| 0 | 8 | magia `ALBAFS02` |
| 8 | 4 | versión (2) |
| 12 | 4 | sectores totales |
| 16 | 8 | generación de la copia (empieza en 1) |
| 24 | 4 | crc-32 ieee de los ocho sectores de directorio |
| 28 | 4 | crc-32 ieee de los primeros 28 bytes de esta cabecera |
| 32 | 480 | reservado, en cero |

el directorio conserva el tamaño y los primeros 52 bytes de las entradas v1.
los bytes 52–59 contienen el identificador del archivo, distinto de cero, y los
bytes 60–63 se reservan en cero. una entrada libre tiene los 64 bytes en cero.
cada archivo nuevo recibe la generación del commit que lo creó. el identificador
se conserva al anexar y evita que un descriptor abierto apunte a un archivo
nuevo tras borrar el anterior. los archivos vacíos llevan sector de inicio cero.

al montar se validan ambas copias por separado. se comprueban sumas, capacidad,
campos reservados, nombres utf-8 únicos, identificadores únicos y extensiones
dentro de la zona de datos sin solapamiento. se elige la generación válida más
alta. si ambas tienen la misma generación, su contenido debe coincidir. si
ninguna copia es válida, el montaje devuelve un error.

### asignación y consistencia

se usa una extensión contigua por archivo. el espacio libre se deduce de las
extensiones del directorio activo, así se evita escribir un mapa de bits cada
vez que cambia un archivo. las lecturas y escrituras contiguas se agrupan. un
anexo copia el archivo a una extensión libre y agrega los bytes al final. esto
cuesta más para archivos grandes y puede fallar si falta espacio contiguo.

cada mutación se serializa con las lecturas y demás mutaciones del sistema de
archivos. si hay datos nuevos, se escriben en sectores libres respecto del
directorio activo y se vacía la caché del dispositivo. luego se escribe el
directorio en la copia inactiva, seguido de su cabecera. al completar el
vaciado final se publica el directorio en memoria. si se corta la operación
entre estos pasos, los datos de la última generación activa siguen intactos y
el próximo montaje descarta cualquier copia con metadatos incompletos.

las mutaciones requieren un dispositivo escribible con soporte de vaciado
virtio-blk. una falla de escritura o vaciado deja el montaje en modo de solo
lectura hasta reiniciar para volver a elegir la copia válida. el paquete
actual de windows conserva el disco en modo de solo lectura.
