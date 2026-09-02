[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$Repository = "mroxso/FPVEditor",
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "FPVEditor\bin")
)

$ErrorActionPreference = "Stop"

if (-not [Environment]::Is64BitOperatingSystem) {
    throw "fpv-cli is currently available only for 64-bit Windows."
}

$assetName = "fpv-cli-windows-x64.zip"
$checksumsName = "fpv-cli-checksums.txt"
if ($Version -eq "latest") {
    $releaseUrl = "https://github.com/$Repository/releases/latest/download"
} else {
    $releaseUrl = "https://github.com/$Repository/releases/download/$Version"
}

$temporaryDir = Join-Path ([System.IO.Path]::GetTempPath()) ("fpv-cli-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $temporaryDir | Out-Null

try {
    $archivePath = Join-Path $temporaryDir $assetName
    $checksumsPath = Join-Path $temporaryDir $checksumsName

    Write-Host "Downloading $assetName..."
    Invoke-WebRequest -Uri "$releaseUrl/$assetName" -OutFile $archivePath
    Invoke-WebRequest -Uri "$releaseUrl/$checksumsName" -OutFile $checksumsPath

    $checksumLine = Get-Content $checksumsPath | Where-Object { $_ -match ("\s" + [regex]::Escape($assetName) + "$") } | Select-Object -First 1
    if (-not $checksumLine) {
        throw "No checksum for $assetName was found in $checksumsName."
    }
    $expectedChecksum = ($checksumLine -split "\s+")[0].ToLowerInvariant()
    $actualChecksum = (Get-FileHash -Algorithm SHA256 -Path $archivePath).Hash.ToLowerInvariant()
    if ($actualChecksum -ne $expectedChecksum) {
        throw "Checksum verification failed for $assetName."
    }

    Expand-Archive -Path $archivePath -DestinationPath $temporaryDir -Force
    $binaryPath = Join-Path $temporaryDir "fpv.exe"
    if (-not (Test-Path $binaryPath)) {
        throw "The release archive does not contain fpv.exe."
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path $binaryPath -Destination (Join-Path $InstallDir "fpv.exe") -Force

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $pathEntries = @($userPath -split ";" | Where-Object { $_ })
    if ($pathEntries -notcontains $InstallDir) {
        [Environment]::SetEnvironmentVariable("Path", (($pathEntries + $InstallDir) -join ";"), "User")
    }
    $env:Path = "$InstallDir;$env:Path"

    Write-Host "Installed fpv to $(Join-Path $InstallDir 'fpv.exe')"
    Write-Host "Open a new terminal before using fpv."

    $missingTools = @()
    if (-not (Get-Command ffmpeg -ErrorAction SilentlyContinue)) { $missingTools += "ffmpeg" }
    if (-not (Get-Command ffprobe -ErrorAction SilentlyContinue)) { $missingTools += "ffprobe" }
    if ($missingTools.Count -gt 0) {
        Write-Warning "fpv was installed, but $($missingTools -join ', ') was not found."
        Write-Host "Install FFmpeg with: winget install Gyan.FFmpeg"
        Write-Host "FFmpeg and FFprobe are required for media probing and export."
    }
} finally {
    Remove-Item -Path $temporaryDir -Recurse -Force -ErrorAction SilentlyContinue
}
