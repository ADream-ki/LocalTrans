param(
  [string]$CliPath = ".\release-assets\localtrans-portable\localtrans-cli.exe",
  [string]$AudioFile = ".\tmp_e2e_en2zh.wav",
  [int]$Rounds = 8,
  [int]$SessionSeconds = 120,
  [string]$OutDir = ".\tmp_stress"
)

$ErrorActionPreference = "Stop"

if (!(Test-Path $CliPath)) { throw "CLI not found: $CliPath" }
if (!(Test-Path $AudioFile)) { throw "Audio file not found: $AudioFile" }

New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
$sessionLog = Join-Path $OutDir "session_monitor.jsonl"
$e2eLog = Join-Path $OutDir "e2e_results.jsonl"
Remove-Item $sessionLog,$e2eLog -ErrorAction SilentlyContinue

function Invoke-CliJson {
  param([string[]]$CmdArgs)
  $raw = & $CliPath @CmdArgs 2>&1
  $txt = ($raw | Out-String).Trim()
  if ([string]::IsNullOrWhiteSpace($txt)) { return $null }
  try { return $txt | ConvertFrom-Json } catch { return $txt }
}

function Write-JsonLine {
  param([string]$Path, [object]$Obj)
  ($Obj | ConvertTo-Json -Depth 8 -Compress) | Add-Content -Path $Path -Encoding UTF8
}

# 1) Session stability monitor
& $CliPath session-stop | Out-Null
Start-Sleep -Milliseconds 500
& $CliPath session-start | Out-Null
Start-Sleep -Seconds 2

$start = Get-Date
$end = $start.AddSeconds($SessionSeconds)
$aliveCount = 0
$expiredCount = 0
$statusCounts = @{}

while ((Get-Date) -lt $end) {
  $st = Invoke-CliJson @("session-status")
  $stats = Invoke-CliJson @("session-stats")
  $now = (Get-Date).ToString("o")
  $entry = [ordered]@{
    ts = $now
    status = $st
    metrics = $stats
  }
  Write-JsonLine -Path $sessionLog -Obj $entry

  $s = ""
  if ($st -is [string]) {
    $s = "string"
  } elseif ($null -ne $st.status) {
    $s = [string]$st.status
  } elseif ($null -ne $st.lastKnown.status) {
    $s = [string]$st.lastKnown.status
  }
  if (-not $statusCounts.ContainsKey($s)) { $statusCounts[$s] = 0 }
  $statusCounts[$s]++

  if ($s -in @("running","starting","paused")) { $aliveCount++ }
  if ($null -ne $st.note -and [string]$st.note -like "*heartbeat expired*") { $expiredCount++ }

  Start-Sleep -Seconds 2
}

& $CliPath session-stop | Out-Null
Start-Sleep -Seconds 1

# 2) Real audio e2e rounds
$ok = 0
$fail = 0
$latencies = @()
for ($i = 1; $i -le $Rounds; $i++) {
  $outWav = Join-Path $OutDir ("round_{0:00}.wav" -f $i)
  $t0 = Get-Date
  $raw = & $CliPath e2e --file $AudioFile --source en --target zh --out-wav $outWav 2>&1
  $txt = ($raw | Out-String).Trim()
  $success = $LASTEXITCODE -eq 0 -and (Test-Path $outWav)
  $lat = $null
  if ($txt -match "Latency\(ms\): asr=(\d+) translate=(\d+) tts=(\d+) total=(\d+)") {
    $lat = [ordered]@{
      asr = [int]$Matches[1]
      translate = [int]$Matches[2]
      tts = [int]$Matches[3]
      total = [int]$Matches[4]
    }
    $latencies += $lat.total
  }
  if ($success) { $ok++ } else { $fail++ }
  $entry = [ordered]@{
    round = $i
    success = $success
    ts = $t0.ToString("o")
    latency = $lat
    output_wav = $outWav
    output = $txt
  }
  Write-JsonLine -Path $e2eLog -Obj $entry
}

$avg = 0
$p95 = 0
if ($latencies.Count -gt 0) {
  $sorted = $latencies | Sort-Object
  $avg = [int][Math]::Round((($latencies | Measure-Object -Average).Average), 0)
  $idx = [Math]::Max([int][Math]::Ceiling($sorted.Count * 0.95) - 1, 0)
  $p95 = [int]$sorted[$idx]
}

$summary = [ordered]@{
  session = [ordered]@{
    duration_sec = $SessionSeconds
    alive_polls = $aliveCount
    heartbeat_expired_polls = $expiredCount
    status_counts = $statusCounts
    log = (Resolve-Path $sessionLog).Path
  }
  e2e = [ordered]@{
    rounds = $Rounds
    success = $ok
    failed = $fail
    avg_total_latency_ms = $avg
    p95_total_latency_ms = $p95
    log = (Resolve-Path $e2eLog).Path
  }
}

$summary | ConvertTo-Json -Depth 8
