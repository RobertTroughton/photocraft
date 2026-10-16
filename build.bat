@echo off
setlocal EnableExtensions

where cargo >nul 2>&1
if not errorlevel 1 goto build

if exist "%USERPROFILE%\.cargo\bin\cargo.exe" (
    set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
    goto build
)

echo Cargo was not found. Installing Rust and Cargo...
set "RUSTUP_INSTALLER=%TEMP%\rustup-init.exe"
powershell -NoProfile -ExecutionPolicy Bypass -Command "try { Invoke-WebRequest -Uri 'https://win.rustup.rs/x86_64' -OutFile $env:RUSTUP_INSTALLER -ErrorAction Stop } catch { Write-Error $_; exit 1 }"
if errorlevel 1 goto install_failed

"%RUSTUP_INSTALLER%" -y --no-modify-path
if errorlevel 1 goto install_failed
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

:build
where cargo >nul 2>&1
if errorlevel 1 goto install_failed

echo Building PhotoCraft...
cargo build --release -p photocraft
if errorlevel 1 goto build_failed

echo.
echo PhotoCraft build completed successfully.
goto finish

:install_failed
echo.
echo Rust/Cargo installation failed. Check the installer errors above.
goto finish

:build_failed
echo.
echo PhotoCraft build failed. See errors above.

:finish
pause
endlocal
