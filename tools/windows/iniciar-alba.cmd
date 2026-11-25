@echo off
setlocal
cd /d "%~dp0"
set "QEMU=qemu-system-aarch64.exe"
where "%QEMU%" >nul 2>&1
if errorlevel 1 set "QEMU=%ProgramFiles%\qemu\qemu-system-aarch64.exe"
if not exist "%QEMU%" (
    where "%QEMU%" >nul 2>&1
    if errorlevel 1 (
        echo no se encontro qemu-system-aarch64.exe. instala qemu para windows.
        echo https://www.qemu.org/download/#windows
        pause
        exit /b 1
    )
)
echo alba: consola arm64 en qemu. para salir, pulsa ctrl+a y luego x.
"%QEMU%" -machine virt -accel tcg -cpu cortex-a72 -m 128M -nographic -global virtio-mmio.force-legacy=false -drive file=alba-disk.img,if=none,format=raw,readonly=on,id=hd0 -device virtio-blk-device,drive=hd0 -kernel alba.bin
if errorlevel 1 (
    echo qemu termino con un error.
    pause
    exit /b 1
)
