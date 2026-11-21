# Bundle a release build with the CEF runtime into dist\Emerald, a portable zip and an
# Inno Setup installer.
#   $env:CEF_PATH = "$env:USERPROFILE\.local\share\cef"; .\scripts\package-windows.ps1 -Version 0.1.0
param([string]$Version = "0.1.0")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $env:CEF_PATH) { throw "Set CEF_PATH to an exported CEF directory" }
$out = Join-Path $root "dist\Emerald"
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item (Join-Path $root "target\release\emerald.exe") $out
$skip = @("include", "cmake", "libcef_dll", "CMakeLists.txt", "archive.json")
Get-ChildItem $env:CEF_PATH | Where-Object { $skip -notcontains $_.Name -and $_.Extension -ne ".lib" } | ForEach-Object {
  Copy-Item $_.FullName -Destination $out -Recurse -Force
}
Copy-Item (Join-Path $root "README.md") $out
Copy-Item (Join-Path $root "ui\fonts\OpenDyslexic-OFL.txt") $out
foreach ($l in @("LICENSE-MIT", "LICENSE-APACHE")) { if (Test-Path (Join-Path $root $l)) { Copy-Item (Join-Path $root $l) $out } }
$zip = Join-Path $root "dist\Emerald-$Version-portable-x64.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path $out -DestinationPath $zip
$iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe"
if (Test-Path $iscc) {
  & $iscc "/DAppVersion=$Version" "/DSourceDir=$out" (Join-Path $root "packaging\windows\emerald.iss")
  if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }
} else {
  Write-Warning "Inno Setup not found; skipped the installer (portable zip was created)."
}
Get-ChildItem (Join-Path $root "dist") -File | Format-Table Name, @{n = "MB"; e = { [math]::Round($_.Length / 1MB, 1) } }
