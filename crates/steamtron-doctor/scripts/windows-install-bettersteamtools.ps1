param(
    [Parameter(Mandatory = $true)][string]$SteamRoot,
    [Parameter(Mandatory = $true)][string]$Log,
    [Parameter(Mandatory = $true)][string]$Marker
)

$ErrorActionPreference = "Stop"
$archiveUrl = "https://github.com/madoiscool/BetterSteamTools/releases/download/v1.0.9/OpenSteamTool-v1.0.9-Release.zip"
$expectedSha256 = "af1affe62d2bd4359c086de983d12a5877fe5b026a2526343d53578dce2d366f"
$temp = Join-Path $env:TEMP ("steamtron-opensteamtool-" + [guid]::NewGuid().ToString("N"))
$exitCode = 1

function Write-Log([string]$message) {
    Add-Content -LiteralPath $Log -Value $message
}

try {
    if (Get-Process -Name steam -ErrorAction SilentlyContinue) {
        throw "Steam is running. Exit Steam completely and retry."
    }
    $steamRootFull = [System.IO.Path]::GetFullPath($SteamRoot)
    if (-not (Test-Path -LiteralPath (Join-Path $steamRootFull "steam.exe"))) {
        throw "Steam root does not contain steam.exe: $steamRootFull"
    }

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
    foreach ($name in @("OpenSteamTool.dll", "dwmapi.dll", "xinput1_4.dll")) {
        $source = Join-Path (Join-Path $temp "release") $name
        if (-not (Test-Path -LiteralPath $source)) {
            throw "Missing $name in the release archive"
        }
        Copy-Item -LiteralPath $source -Destination (Join-Path $steamRootFull $name) -Force
    }
    New-Item -ItemType Directory -Path (Join-Path $steamRootFull "config\lua") -Force | Out-Null
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
