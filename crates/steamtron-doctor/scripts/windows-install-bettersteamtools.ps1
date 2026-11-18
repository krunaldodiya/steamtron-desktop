param(
    [Parameter(Mandatory = $true)][string]$SteamRoot,
    [Parameter(Mandatory = $true)][string]$Log,
    [Parameter(Mandatory = $true)][string]$Marker
)

$ErrorActionPreference = "Stop"
$archiveUrl = "https://github.com/madoiscool/BetterSteamTools/releases/download/v1.0.9/OpenSteamTool-v1.0.9-Release.zip"
$expectedSha256 = "af1affe62d2bd4359c086de983d12a5877fe5b026a2526343d53578dce2d366f"
# Fixed staging directory so it can be excluded before the archive is written.
$temp = Join-Path $env:TEMP "steamtron-opensteamtool"
$exitCode = 1
$dlls = @("OpenSteamTool.dll", "dwmapi.dll", "xinput1_4.dll")

function Write-Log([string]$message) {
    Add-Content -LiteralPath $Log -Value $message
}

function Add-ScopedExclusions([string[]]$paths) {
    if (-not (Get-Command Add-MpPreference -ErrorAction SilentlyContinue)) {
        Write-Log "warning: Microsoft Defender cmdlets are unavailable; add antivirus exclusions manually."
        return
    }
    foreach ($path in $paths) {
        try {
            Add-MpPreference -ExclusionPath $path -ErrorAction Stop
            Write-Log "Windows Security excluded $path"
        } catch {
            Write-Log ("warning: Windows Security exclusion failed for " + $path + ": " + $_.Exception.Message)
        }
    }
}

try {
    if (Get-Process -Name steam -ErrorAction SilentlyContinue) {
        throw "Steam is running. Exit Steam completely and retry."
    }
    $steamRootFull = [System.IO.Path]::GetFullPath($SteamRoot)
    if (-not (Test-Path -LiteralPath (Join-Path $steamRootFull "steam.exe"))) {
        throw "Steam root does not contain steam.exe: $steamRootFull"
    }

    $openSteamTool = Join-Path $steamRootFull "opensteamtool"
    $stplugIn = Join-Path $steamRootFull "config\stplug-in"
    $targets = @($dlls | ForEach-Object { Join-Path $steamRootFull $_ })
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue

    # Exclude before downloading or writing anything: Defender can quarantine a
    # DLL while it is still in the staging folder or during the copy.
    Add-ScopedExclusions (@($temp, $openSteamTool, $stplugIn) + $targets)

    New-Item -ItemType Directory -Path $temp -Force | Out-Null
    $zip = Join-Path $temp "OpenSteamTool.zip"
    Write-Log "Downloading BetterSteamTools 1.0.9 (OpenSteamTool fork)"
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri $archiveUrl -OutFile $zip -UseBasicParsing
    $actualSha256 = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualSha256 -ne $expectedSha256) {
        throw "OpenSteamTool archive checksum mismatch"
    }
    Expand-Archive -LiteralPath $zip -DestinationPath (Join-Path $temp "release") -Force

    Write-Log "Installing into $steamRootFull"
    foreach ($name in $dlls) {
        $source = Join-Path (Join-Path $temp "release") $name
        if (-not (Test-Path -LiteralPath $source)) {
            throw "Missing $name in the release archive"
        }
        $destination = Join-Path $steamRootFull $name
        Copy-Item -LiteralPath $source -Destination $destination -Force
        $installed = Get-Item -LiteralPath $destination -ErrorAction SilentlyContinue
        if (-not $installed -or $installed.Length -le 0) {
            throw "Antivirus removed $destination during installation. Allow the excluded path and retry."
        }
    }
    New-Item -ItemType Directory -Path (Join-Path $steamRootFull "config\lua") -Force | Out-Null
    New-Item -ItemType Directory -Path $stplugIn -Force | Out-Null
    New-Item -ItemType Directory -Path $openSteamTool -Force | Out-Null
    $exitCode = 0
    Write-Log "BetterSteamTools 1.0.9 installed."
} catch {
    Write-Log ("error: " + $_.Exception.Message)
    $exitCode = 1
} finally {
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
    "installer exit: $exitCode" | Add-Content -LiteralPath $Log
    Set-Content -LiteralPath $Marker -Value $exitCode -NoNewline
}

exit $exitCode
