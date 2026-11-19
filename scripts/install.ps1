# Install the `lt` CLI from GitHub Releases (Windows).
#
# Usage:
#   irm https://raw.githubusercontent.com/<owner>/<repo>/main/scripts/install.ps1 | iex
#   $env:LT_VERSION = 'v0.1.0'; irm ... | iex
#   $env:LT_REPO = 'myorg/load-testing'; irm ... | iex
#
# Env:
#   LT_REPO         owner/repo (required unless DefaultRepo is set below)
#   LT_VERSION      tag (e.g. v0.1.0); default: latest release
#   LT_INSTALL_DIR  install directory (default: %LOCALAPPDATA%\lt\bin)
#   LT_FORCE        set to '1' to overwrite

$ErrorActionPreference = 'Stop'

$DefaultRepo = 'Bipolaire/load-testing'

function Write-Info([string]$Message) { Write-Host "==> $Message" }
function Write-Warn([string]$Message) { Write-Warning $Message }

$Repo = if ($env:LT_REPO) { $env:LT_REPO } elseif ($env:GITHUB_REPOSITORY) { $env:GITHUB_REPOSITORY } else { $DefaultRepo }
if ([string]::IsNullOrWhiteSpace($Repo)) {
    throw 'Set $env:LT_REPO = ''owner/repo'' (GitHub repository that hosts Releases)'
}

$Arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant()
switch ($Arch) {
    'x64' { $ArchKey = 'x86_64' }
    'arm64' { $ArchKey = 'aarch64' }
    default { throw "Unsupported architecture: $Arch" }
}

# Ship x86_64 Windows builds today; refuse other hosts clearly.
if ($ArchKey -ne 'x86_64') {
    throw "No Windows CLI build for $ArchKey yet (only x86_64-pc-windows-msvc)"
}

$Target = 'x86_64-pc-windows-msvc'
$Version = $env:LT_VERSION
if ([string]::IsNullOrWhiteSpace($Version)) {
    Write-Info "Resolving latest release for $Repo"
    $headers = @{ 'User-Agent' = 'lt-install' }
    if ($env:GITHUB_TOKEN) { $headers['Authorization'] = "Bearer $($env:GITHUB_TOKEN)" }
    $latest = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers $headers
    $Version = $latest.tag_name
}
if (-not $Version.StartsWith('v')) { $Version = "v$Version" }

$Archive = "lt-$Version-$Target.zip"
$BaseUrl = "https://github.com/$Repo/releases/download/$Version"
$DownloadUrl = "$BaseUrl/$Archive"
$ChecksumUrl = "$BaseUrl/SHA256SUMS"

$InstallDir = if ($env:LT_INSTALL_DIR) { $env:LT_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'lt\bin' }
$Dest = Join-Path $InstallDir 'lt.exe'

$Temp = Join-Path ([System.IO.Path]::GetTempPath()) ("lt-install-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $Temp | Out-Null
try {
    $ArchivePath = Join-Path $Temp $Archive
    Write-Info "Downloading $Archive"
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ArchivePath -UseBasicParsing

    try {
        $SumsPath = Join-Path $Temp 'SHA256SUMS'
        Invoke-WebRequest -Uri $ChecksumUrl -OutFile $SumsPath -UseBasicParsing
        $line = Get-Content $SumsPath | Where-Object { $_ -match [regex]::Escape($Archive) } | Select-Object -First 1
        if ($line) {
            $expected = ($line -split '\s+')[0].Trim().ToLowerInvariant()
            $actual = (Get-FileHash -Algorithm SHA256 -Path $ArchivePath).Hash.ToLowerInvariant()
            if ($actual -ne $expected) {
                throw "Checksum mismatch for $Archive"
            }
            Write-Info 'Checksum OK'
        } else {
            Write-Warn "No checksum entry for $Archive in SHA256SUMS"
        }
    } catch {
        if ($_.Exception.Message -like 'Checksum mismatch*') { throw }
        Write-Warn 'SHA256SUMS not found; skipping checksum verification'
    }

    Expand-Archive -Path $ArchivePath -DestinationPath $Temp -Force
    $Binary = Join-Path $Temp 'lt.exe'
    if (-not (Test-Path $Binary)) {
        throw 'Archive did not contain lt.exe'
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    if ((Test-Path $Dest) -and $env:LT_FORCE -ne '1') {
        Write-Warn "Overwriting existing $Dest"
    }
    Copy-Item -Path $Binary -Destination $Dest -Force

    Write-Host "Installed lt $Version -> $Dest" -ForegroundColor Green

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = @()
    if ($userPath) { $parts = $userPath -split ';' | Where-Object { $_ -and $_.Trim() -ne '' } }
    if ($parts -notcontains $InstallDir) {
        $newPath = if ($parts.Count -gt 0) { ($parts + $InstallDir) -join ';' } else { $InstallDir }
        [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
        $env:Path = "$InstallDir;$env:Path"
        Write-Info "Added $InstallDir to user PATH (new shells will pick it up)"
    }

    & $Dest --version
} finally {
    Remove-Item -Recurse -Force $Temp -ErrorAction SilentlyContinue
}
