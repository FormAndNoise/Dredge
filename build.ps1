# build.ps1
# This script compiles the Rust engine and packages it with the Python GUI into a unified binary.

Write-Host "Building the Rust engine..."
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Error "Failed to build the Rust engine."
    exit $LASTEXITCODE
}

Write-Host "Copying the compiled executable..."
Copy-Item -Path "target\release\rustore_mp4.exe" -Destination "rustore_mp4.exe" -Force

Write-Host "Packaging the unified binary with PyInstaller..."
# Ensure PyInstaller is installed
python -m pip install pyinstaller

# Build a single-file executable that bundles rustore_mp4.exe internally.
pyinstaller --noconfirm --onefile --windowed --icon "Rustoremp4favicon.ico" --add-data "rustore_mp4.exe;." --add-data "Rustoremp4favicon.png;." --name "rustore_mp4_gui" rustore_mp4_gui.py
if ($LASTEXITCODE -ne 0) {
    Write-Error "Failed to build the unified GUI."
    exit $LASTEXITCODE
}

Write-Host "Success! Your unified binary is located in the 'dist' folder:"
Get-ChildItem -Path "dist" | Select-Object Name
