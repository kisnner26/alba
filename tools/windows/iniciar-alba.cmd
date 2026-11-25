@echo off
setlocal DisableDelayedExpansion
pushd "%~dp0" || (
    echo no se pudo abrir la carpeta de alba.
    pause
    exit /b 1
)

if not exist "alba.bin" goto :faltan_archivos
if not exist "alba-fabrica.img" goto :faltan_archivos

set "QEMU="
for %%I in (qemu-system-aarch64.exe) do set "QEMU=%%~$PATH:I"
if not defined QEMU if exist "%ProgramFiles%\qemu\qemu-system-aarch64.exe" (
    set "QEMU=%ProgramFiles%\qemu\qemu-system-aarch64.exe"
)
if not defined QEMU (
    echo no se encontro qemu-system-aarch64.exe en el path ni en program files\qemu.
    echo instala qemu para windows de 64 bits: https://www.qemu.org/download/#windows
    goto :error
)

if exist "alba-disk.img" goto :disco_listo
set "TEMP_DISK=alba-inicio-%RANDOM%-%RANDOM%.img"
copy /b "alba-fabrica.img" "%TEMP_DISK%" >nul || goto :error_disco
move /y "%TEMP_DISK%" "alba-disk.img" >nul || goto :error_disco

:disco_listo
set "DISPLAY=none"
set "GRAPHICS="
if /i "%~1"=="--console" goto :arrancar
set "DISPLAY_LIST=%TEMP%\alba-qemu-%RANDOM%-%RANDOM%.txt"
"%QEMU%" -display help > "%DISPLAY_LIST%" 2>&1
findstr /i /x /c:"gtk" "%DISPLAY_LIST%" >nul && set "DISPLAY=gtk"
findstr /i /x /c:"sdl" "%DISPLAY_LIST%" >nul && set "DISPLAY=sdl"
del "%DISPLAY_LIST%"
if "%DISPLAY%"=="none" (
    echo esta compilacion de qemu no ofrece sdl ni gtk. se usara la consola.
) else (
    set "GRAPHICS=-device virtio-gpu-device -device virtio-keyboard-device -device virtio-tablet-device"
)

:arrancar
echo la salida serie se guarda en alba.log. para salir, pulsa ctrl+a y luego x en la consola.
"%QEMU%" -machine virt -accel tcg -cpu cortex-a72 -m 128M ^
    -display %DISPLAY% -chardev stdio,id=alba,mux=on,signal=off,logfile=alba.log,logappend=off ^
    -serial chardev:alba -monitor none -global virtio-mmio.force-legacy=false ^
    -drive file=alba-disk.img,if=none,format=raw,id=hd0 ^
    -device virtio-blk-device,drive=hd0 %GRAPHICS% -kernel alba.bin
set "RESULT=%ERRORLEVEL%"
if "%RESULT%"=="0" (
    popd
    exit /b 0
)
echo qemu termino con codigo %RESULT%. revisa el mensaje anterior.
goto :error

:error_disco
if exist "%TEMP_DISK%" del "%TEMP_DISK%"
echo no se pudo crear el disco de trabajo. usa una carpeta con permiso de escritura.
goto :error

:faltan_archivos
echo faltan alba.bin o alba-fabrica.img. descomprime todo el zip en la misma carpeta.
:error
popd
pause
exit /b 1
