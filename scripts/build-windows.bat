@echo off
setlocal
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" x64
set "Path=C:\Users\hangello\.cargo\bin;%Path%"
set "CARGO_TARGET_DIR=C:\Users\hangello\.cargo\target\load-testing"
cd /d "%~dp0.."
cargo build --release --target x86_64-pc-windows-msvc -p lt-app -p lt-cli
if errorlevel 1 exit /b 1
echo.
echo Binaires :
echo   %CARGO_TARGET_DIR%\x86_64-pc-windows-msvc\release\load-testing.exe
echo   %CARGO_TARGET_DIR%\x86_64-pc-windows-msvc\release\lt.exe
