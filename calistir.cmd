@echo off
rem Gelistirme: Sqywein'i derler, kurulum dosyasini uretir ve gelistirme kopyasini acar.
rem Kullanim: calistir.cmd [adres ...]   ornek: calistir.cmd github.com
rem
rem Gunluk kullandigin Sqywein kurulu olandir (Baslat menusu / masaustu / gorev cubugu).
rem Buradan acilan gelistirme kopyasi ayri veri klasoru kullanir; kurulu surume dokunmaz.
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

rem Her derlemede kurulum dosyasi: cift tikla, kurulu Sqywein guncellensin.
if not exist dist mkdir dist
copy /y "%CARGO_TARGET_DIR%\release\sqywein.exe" "dist\SqyweinSetup.exe" >nul
echo Kurulum dosyasi hazir: %~dp0dist\SqyweinSetup.exe

rem Gelistirme kopyasi: ayri profil, gorev cubugunda ayri simge.
set "SQYWEIN_DATA=%LOCALAPPDATA%\Sqywein-dev"
start "" "%CARGO_TARGET_DIR%\release\sqywein.exe" %*
