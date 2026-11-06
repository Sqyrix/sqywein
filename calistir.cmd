@echo off
rem Sqywein'in Rust surumunu derler (degisiklik varsa) ve acar.
rem Kullanim: calistir.cmd [adres ...]   ornek: calistir.cmd github.com
setlocal
cd /d "%~dp0rust"
set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
if not exist "%CARGO%" set "CARGO=cargo"

echo Sqywein derleniyor...
"%CARGO%" build --release
if errorlevel 1 (
    echo.
    echo Derleme basarisiz oldu. Hatalar yukarida.
    pause
    exit /b 1
)
start "" "%~dp0rust\target\release\sqywein.exe" %*
