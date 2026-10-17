#Requires -Version 5.1

param(
  [string]$InstallDir = "$env:LOCALAPPDATA\Programs\AirDash",
  [string]$Version = "",
  [switch]$AddToPath,
  [switch]$Uninstall,
  [switch]$Help
)

$ErrorActionPreference = "Stop"
# Invoke-WebRequest in Windows PowerShell 5.1 is many times slower while it draws a progress bar.
$ProgressPreference = "SilentlyContinue"

$GitHubRepo = "christianhelle/airdash"
$BinaryName = "airdash.exe"
$ShortcutPath = Join-Path ([Environment]::GetFolderPath("Programs")) "AirDash.lnk"

function Write-Info
{
  param([string]$Message)
  Write-Host "[INFO] $Message" -ForegroundColor Cyan
}

function Write-Success
{
  param([string]$Message)
  Write-Host "[OK] $Message" -ForegroundColor Green
}

function Write-WarningMessage
{
  param([string]$Message)
  Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Write-ErrorMessage
{
  param([string]$Message)
  Write-Host "[ERROR] $Message" -ForegroundColor Red
}

function Show-Usage
{
  Write-Host ""
  Write-Host "AirDash installation script for Windows" -ForegroundColor Blue
  Write-Host ""
  Write-Host "Usage:" -ForegroundColor Yellow
  Write-Host "  irm https://christianhelle.com/airdash/install.ps1 | iex" -ForegroundColor White
  Write-Host ""
  Write-Host "Parameters:" -ForegroundColor Yellow
  Write-Host "  -InstallDir <path>   Installation directory (default: %LOCALAPPDATA%\Programs\AirDash)" -ForegroundColor White
  Write-Host "  -Version <tag>       Install a specific GitHub Release tag instead of the latest" -ForegroundColor White
  Write-Host "  -AddToPath           Also add the installation directory to the user PATH" -ForegroundColor White
  Write-Host "  -Uninstall           Remove AirDash and its Start Menu shortcut" -ForegroundColor White
  Write-Host "  -Help                Show this help" -ForegroundColor White
  Write-Host ""
}

function Get-Target
{
  $architecture = $null
  try
  {
    $architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
  } catch
  {
    # Older .NET Framework builds lack RuntimeInformation. PROCESSOR_ARCHITEW6432 holds the real CPU
    # when this is a 32-bit PowerShell.
    $architecture = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
  }

  switch ($architecture)
  {
    { $_ -in "X64", "AMD64" }
    {
      return "x86_64-pc-windows-msvc"
    }
    { $_ -eq "Arm64" }
    {
      return "aarch64-pc-windows-msvc"
    }
    default
    {
      throw "Unsupported Windows architecture: $architecture"
    }
  }
}

function Get-DownloadUrl
{
  param([string]$ArchiveName)

  if ([string]::IsNullOrWhiteSpace($Version))
  {
    return "https://github.com/$GitHubRepo/releases/latest/download/$ArchiveName"
  }
  return "https://github.com/$GitHubRepo/releases/download/$Version/$ArchiveName"
}

function Test-IsInPath
{
  param([string]$Directory, [string]$PathValue)

  $pathDirs = $PathValue -split ';' | ForEach-Object { $_.Trim('"').TrimEnd('\') }
  return $pathDirs -contains $Directory.TrimEnd('\')
}

function Add-ToUserPath
{
  param([string]$Directory)

  $userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
  if (Test-IsInPath -Directory $Directory -PathValue $userPath)
  {
    Write-Info "Directory already in PATH: $Directory"
    return
  }

  if ([string]::IsNullOrWhiteSpace($userPath))
  {
    $userPath = $Directory
  } elseif ($userPath.EndsWith(';'))
  {
    $userPath += $Directory
  } else
  {
    $userPath += ";$Directory"
  }

  [Environment]::SetEnvironmentVariable("PATH", $userPath, "User")
  $env:PATH += ";$Directory"

  Write-Success "Added to user PATH: $Directory"
  Write-WarningMessage "Restart your terminal for PATH changes to take effect"
}

function Remove-FromUserPath
{
  param([string]$Directory)

  $userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
  if (-not (Test-IsInPath -Directory $Directory -PathValue $userPath))
  {
    return
  }

  $kept = $userPath -split ';' | Where-Object { $_ -and ($_.Trim('"').TrimEnd('\') -ne $Directory.TrimEnd('\')) }
  [Environment]::SetEnvironmentVariable("PATH", ($kept -join ';'), "User")
  Write-Info "Removed from user PATH: $Directory"
}

# Windows keeps a running executable locked, but lets it be renamed. Moving the old one aside
# lets an update go through while AirDash is open; the leftover is removed on the next install.
function Install-Binary
{
  param([string]$SourceBinary, [string]$TargetBinary)

  $oldBinary = "$TargetBinary.old"
  if (Test-Path $oldBinary)
  {
    Remove-Item -Path $oldBinary -Force -ErrorAction SilentlyContinue
  }
  if (Test-Path $TargetBinary)
  {
    Move-Item -Path $TargetBinary -Destination $oldBinary -Force
  }
  Copy-Item -Path $SourceBinary -Destination $TargetBinary -Force
  Remove-Item -Path $oldBinary -Force -ErrorAction SilentlyContinue
}

function New-StartMenuShortcut
{
  param([string]$TargetBinary)

  $shell = New-Object -ComObject WScript.Shell
  $shortcut = $shell.CreateShortcut($ShortcutPath)
  $shortcut.TargetPath = $TargetBinary
  $shortcut.WorkingDirectory = Split-Path $TargetBinary
  $shortcut.Description = "Open source dashboard for Airthings air quality monitors"
  $shortcut.Save()
  Write-Info "Added AirDash to the Start Menu"
}

function Install-AirDash
{
  $archiveName = "airdash-$(Get-Target).zip"
  $tempDir = Join-Path $env:TEMP "airdash-install-$(Get-Random)"
  $archivePath = Join-Path $tempDir $archiveName

  try
  {
    New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

    Write-Info "Downloading $archiveName..."
    $invokeWebRequestParams = @{
      Uri = Get-DownloadUrl -ArchiveName $archiveName
      OutFile = $archivePath
    }
    if ($PSVersionTable.PSVersion.Major -lt 6)
    {
      $invokeWebRequestParams.UseBasicParsing = $true
    }
    try
    {
      Invoke-WebRequest @invokeWebRequestParams
    } catch
    {
      if (-not [string]::IsNullOrWhiteSpace($Version))
      {
        throw "Failed to download $archiveName. Check that release '$Version' exists: https://github.com/$GitHubRepo/releases"
      }
      throw "Failed to download $archiveName from GitHub Releases: $($_.Exception.Message)"
    }

    Write-Info "Extracting archive..."
    $extractDir = Join-Path $tempDir "files"
    Expand-Archive -Path $archivePath -DestinationPath $extractDir -Force

    $sourceBinary = Get-ChildItem -Path $extractDir -Filter $BinaryName -File -Recurse | Select-Object -First 1
    if ($null -eq $sourceBinary)
    {
      throw "The downloaded archive did not contain $BinaryName."
    }

    if (-not (Test-Path $InstallDir))
    {
      Write-Info "Creating installation directory: $InstallDir"
      New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    $targetBinary = Join-Path $InstallDir $BinaryName
    Write-Info "Installing to $InstallDir..."
    Install-Binary -SourceBinary $sourceBinary.FullName -TargetBinary $targetBinary

    return $targetBinary
  } finally
  {
    if (Test-Path $tempDir)
    {
      Remove-Item -Path $tempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
  }
}

function Uninstall-AirDash
{
  Write-Info "Removing AirDash..."

  $targetBinary = Join-Path $InstallDir $BinaryName
  foreach ($file in @($targetBinary, "$targetBinary.old", $ShortcutPath))
  {
    if (Test-Path $file)
    {
      Remove-Item -Path $file -Force
    }
  }
  Remove-FromUserPath -Directory $InstallDir

  # Only remove the folder when nothing else was put in it.
  if ((Test-Path $InstallDir) -and -not (Get-ChildItem -Path $InstallDir -Force))
  {
    Remove-Item -Path $InstallDir -Force
  }

  Write-Success "AirDash has been removed. Your settings and saved dashboard are kept."
}

function Main
{
  if ($Help)
  {
    Show-Usage
    return
  }

  if ($Uninstall)
  {
    Uninstall-AirDash
    return
  }

  # GitHub requires TLS 1.2, which Windows PowerShell 5.1 does not always enable by default.
  [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

  Write-Info "Starting AirDash installation for Windows..."
  Write-Info "Installing release: $(if ([string]::IsNullOrWhiteSpace($Version)) { 'latest' } else { $Version })"

  $targetBinary = Install-AirDash
  New-StartMenuShortcut -TargetBinary $targetBinary

  if ($AddToPath)
  {
    Add-ToUserPath -Directory $InstallDir
  }

  Write-Success "Installation complete."
  Write-Info "Start AirDash from the Start Menu, or run: $targetBinary"
}

# No `exit` here: under `irm | iex` it would close the user's PowerShell window.
try
{
  Main
} catch
{
  Write-ErrorMessage $_.Exception.Message
}
