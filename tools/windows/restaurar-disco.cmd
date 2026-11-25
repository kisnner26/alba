@echo off
setlocal DisableDelayedExpansion
pushd "%~dp0" || exit /b 1

if not exist "alba-fabrica.img" (
    echo falta alba-fabrica.img. descomprime el paquete completo.
    goto :error
)
echo cierra qemu antes de restaurar. se borraran los cambios del disco de alba.
choice /c SN /n /m "restaurar el disco? [s/n] "
if not "%ERRORLEVEL%"=="1" (
    popd
    exit /b 0
)
set "TEMP_DISK=alba-restauracion-%RANDOM%-%RANDOM%.img"
copy /b /y "alba-fabrica.img" "%TEMP_DISK%" >nul || goto :error
move /y "%TEMP_DISK%" "alba-disk.img" >nul || goto :error
echo disco de alba restaurado.
popd
pause
exit /b 0

:error
if defined TEMP_DISK if exist "%TEMP_DISK%" del "%TEMP_DISK%"
echo no se pudo restaurar. comprueba que qemu este cerrado.
popd
pause
exit /b 1
