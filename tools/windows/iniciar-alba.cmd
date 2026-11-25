@echo off
setlocal DisableDelayedExpansion
pushd "%~dp0" || (
    echo no se pudo abrir la carpeta de alba.
    pause
    exit /b 1
)

if not exist "alba.bin" goto :faltan_archivos
if not exist "alba-disk.img" goto :faltan_archivos

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

set "SERIAL=stdio,id=alba,mux=on,signal=off"
if /i "%~1"=="--log" (
    set "SERIAL=stdio,id=alba,mux=on,signal=off,logfile=alba.log,logappend=off"
    echo guardando la salida serie en alba.log.
)

echo alba: consola arm64 en qemu. para salir, pulsa ctrl+a y luego x.
"%QEMU%" -machine virt -accel tcg -cpu cortex-a72 -m 128M ^
    -display none -chardev "%SERIAL%" -serial chardev:alba -monitor none ^
    -global virtio-mmio.force-legacy=false ^
    -drive file=alba-disk.img,if=none,format=raw,readonly=on,id=hd0 ^
    -device virtio-blk-device,drive=hd0 -kernel alba.bin
set "RESULT=%ERRORLEVEL%"
if "%RESULT%"=="0" (
    popd
    exit /b 0
)
echo qemu termino con codigo %RESULT%. revisa el mensaje anterior.
goto :error

:faltan_archivos
echo faltan alba.bin o alba-disk.img. descomprime todo el zip en la misma carpeta.
:error
popd
pause
exit /b 1
