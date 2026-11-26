$ErrorActionPreference = 'Stop'
Set-Location 'C:\Users\krunaldodiya\Downloads'
$exe = 'C:\Program Files (x86)\SteamTron\steamtron-desktop.exe'
$app = 'D:\SteamTronLibrary\steamtronapps'
$base = Join-Path $app 'common\.steamtron-base\Little Nightmares'
$hiddenRoot = Join-Path $app 'common\.steamtron-junction'
$hidden = Join-Path $hiddenRoot 'Little Nightmares'
$targetRel = 'Atlas\Binaries\Win64\LittleNightmares.exe'
$target = Join-Path $hidden $targetRel
$cwd = Join-Path $hidden 'Atlas\Binaries\Win64'
$baseExe = Join-Path $base $targetRel
$fixes = Join-Path $app 'fixes\Little Nightmares'
$log = 'C:\Users\krunaldodiya\AppData\Local\SteamTron\diagnostics\424840\play-hidden-junction.log'
$err = 'C:\Users\krunaldodiya\AppData\Local\SteamTron\diagnostics\424840\play-hidden-junction.stderr.log'
$out = 'C:\Users\krunaldodiya\AppData\Local\SteamTron\diagnostics\424840\play-hidden-junction.stdout.log'
$result = 'C:\Users\krunaldodiya\Downloads\ln1-hidden-junction-result.txt'
function Log([string]$text) { Write-Output $text; Add-Content -LiteralPath $result -Value $text }
function Q([string]$value) { return '"' + $value + '"' }
if (Test-Path -LiteralPath $result) { Remove-Item -LiteralPath $result -Force }
foreach ($p in @($log, $err, $out)) { if (Test-Path -LiteralPath $p) { Remove-Item -LiteralPath $p -Force } }

Log ('started=' + (Get-Date).ToString('o'))
Log ('base_exists=' + (Test-Path -LiteralPath $base))
Log ('base_exe_exists=' + (Test-Path -LiteralPath $baseExe))
if (-not (Test-Path -LiteralPath $baseExe)) { Log 'ERROR: base exe missing'; exit 1 }
$hashBefore = (Get-FileHash -LiteralPath $baseExe -Algorithm SHA256).Hash.ToLower()
Log ('base_hash_before=' + $hashBefore)

Get-CimInstance Win32_Process | Where-Object { $_.Name -match 'LittleNightmares' } | ForEach-Object {
  try { Stop-Process -Id $_.ProcessId -Force -ErrorAction Stop } catch {}
}
Start-Sleep -Seconds 1
if (Test-Path -LiteralPath $hiddenRoot) {
  Get-ChildItem -LiteralPath $hiddenRoot -Force | ForEach-Object {
    if ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) { cmd /c rmdir (Q $_.FullName) 2>$null | Out-Null }
  }
} else {
  New-Item -Path $hiddenRoot -ItemType Directory -Force | Out-Null
}
if (Test-Path -LiteralPath $hidden) {
  $item = Get-Item -LiteralPath $hidden -Force
  if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { cmd /c rmdir (Q $hidden) 2>$null | Out-Null }
}
cmd /c mklink /J (Q $hidden) (Q $base) | Out-Null
Log ('hidden_junction_created=' + (Test-Path -LiteralPath $hidden))
Log ('hidden_target=' + ((Get-Item -LiteralPath $hidden -Force).Target -join ';'))

$argline = @(
  '--steamtron-overlay', '--appid', '424840',
  '--common', (Q $base), '--fixes', (Q $fixes), '--cwd', (Q $cwd),
  '--steam-root', (Q 'C:\Users\krunaldodiya\AppData\Local\steamtron'),
  '--log', (Q $log), '--junction', (Q $hidden), '--base', (Q $base),
  '--', (Q $target)
) -join ' '
$wrapper = Start-Process -FilePath $exe -ArgumentList $argline -PassThru -RedirectStandardError $err -RedirectStandardOutput $out
Log ('wrapper_pid=' + $wrapper.Id)

$seen = @{}
$windows = @{}
$deadline = (Get-Date).AddSeconds(40)
while ((Get-Date) -lt $deadline) {
  Get-CimInstance Win32_Process | Where-Object { $_.Name -match 'LittleNightmares' } | ForEach-Object {
    if (-not $seen.ContainsKey([string]$_.ProcessId)) {
      $seen[[string]$_.ProcessId] = $_.Name
      Log ('proc=' + $_.ProcessId + ' ppid=' + $_.ParentProcessId + ' name=' + $_.Name + ' cmd=' + $_.CommandLine)
    }
  }
  Get-Process -ErrorAction SilentlyContinue | ForEach-Object {
    if ($_.MainWindowTitle -match 'Little Nightmares') { $windows[[string]$_.Id] = $_.MainWindowTitle }
  }
  if ($seen.Count -gt 0) { Start-Sleep -Seconds 8; break }
  if (-not (Get-Process -Id $wrapper.Id -ErrorAction SilentlyContinue)) { break }
  Start-Sleep -Milliseconds 500
}
Log ('distinct_little_nightmares_processes=' + $seen.Count)
$windows.GetEnumerator() | ForEach-Object { Log ('window=' + $_.Value + ' pid=' + $_.Key) }
Log '=== overlay log ==='
Log (Get-Content -LiteralPath $log -ErrorAction SilentlyContinue)
Log '=== stderr ==='
Log (Get-Content -LiteralPath $err -ErrorAction SilentlyContinue)

Log '=== stopping test game ==='
Get-CimInstance Win32_Process | Where-Object { $_.Name -match 'LittleNightmares' } | ForEach-Object {
  try { Stop-Process -Id $_.ProcessId -Force -ErrorAction Stop } catch {}
}
for ($i = 0; $i -lt 30; $i++) {
  $w = Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'steamtron-desktop.exe' -and $_.CommandLine -match '--steamtron-overlay' }
  if (-not $w) { break }
  Start-Sleep -Seconds 1
}
$w = Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'steamtron-desktop.exe' -and $_.CommandLine -match '--steamtron-overlay' }
if ($w) { foreach ($p in $w) { try { Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue } catch {} }; Start-Sleep -Seconds 2 }

$hiddenExists = Test-Path -LiteralPath $hidden
Log ('hidden_exists_after=' + $hiddenExists)
if ($hiddenExists) {
  $item = Get-Item -LiteralPath $hidden -Force
  Log ('hidden_attrs_after=' + $item.Attributes)
  Log ('hidden_target_after=' + ($item.Target -join ';'))
}
$hashAfter = (Get-FileHash -LiteralPath $baseExe -Algorithm SHA256).Hash.ToLower()
Log ('base_hash_after=' + $hashAfter)
Log ('base_unchanged=' + ($hashAfter -eq $hashBefore))

# Cleanup the experiment junction and empty root.
if (Test-Path -LiteralPath $hidden) {
  $item = Get-Item -LiteralPath $hidden -Force
  if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { cmd /c rmdir (Q $hidden) 2>$null | Out-Null }
  elseif (@(Get-ChildItem -LiteralPath $hidden -Force).Count -eq 0) { Remove-Item -LiteralPath $hidden -Force }
}
if (Test-Path -LiteralPath $hiddenRoot) {
  if (@(Get-ChildItem -LiteralPath $hiddenRoot -Force).Count -eq 0) { Remove-Item -LiteralPath $hiddenRoot -Force }
}
Log ('hidden_root_exists_after_cleanup=' + (Test-Path -LiteralPath $hiddenRoot))
Log ('finished=' + (Get-Date).ToString('o'))
