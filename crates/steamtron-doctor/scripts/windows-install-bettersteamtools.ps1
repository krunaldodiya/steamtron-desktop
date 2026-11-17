param(
    [Parameter(Mandatory = $true)][string]$SteamRoot,
    [Parameter(Mandatory = $true)][string]$Log,
    [Parameter(Mandatory = $true)][string]$Marker
)

$ErrorActionPreference = "Stop"
$archiveUrl = "https://github.com/OpenSteam001/OpenSteamTool/releases/download/1.4.8/OpenSteamTool-1.4.8-Release.zip"
$expectedSha256 = "966654604d258d5d5383e72fec616df7957bf86f760c67eeb3d0e18cd882c710"
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
    Write-Log "Downloading OpenSteamTool 1.4.8"
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
    Write-Log "OpenSteamTool installed."
} catch {
    Write-Log ("error: " + $_.Exception.Message)
    $exitCode = 1
} finally {
    Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
    "installer exit: $exitCode" | Add-Content -LiteralPath $Log
    Set-Content -LiteralPath $Marker -Value $exitCode -NoNewline
}

exit $exitCode
