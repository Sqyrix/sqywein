@echo off
rem Sqywein'i derler (degisiklik varsa) ve acar.
rem Kullanim: calistir.cmd [adres ...]   ornek: calistir.cmd github.com
setlocal
cd /d "%~dp0"
set "CARGO=%USERPROFILE%\.cargo\bin\cargo.exe"
if not exist "%CARGO%" set "CARGO=cargo"

rem Derleme dosyalari (birkac GB) proje klasorunde degil, OneDrive disinda tutulur.
set "CARGO_TARGET_DIR=%LOCALAPPDATA%\sqywein-build"

echo Sqywein derleniyor...
"%CARGO%" build --release
if errorlevel 1 (
    echo.
    echo Derleme basarisiz oldu. Hatalar yukarida.
    pause
    exit /b 1
)
start "" "%CARGO_TARGET_DIR%\release\sqywein.exe" %*
