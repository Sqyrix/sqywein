param([int]$BrowserId, [int]$Seconds = 3)
$ErrorActionPreference = 'Stop'
if (!$BrowserId) {
    $browsers = @(Get-Process sqywein -ErrorAction SilentlyContinue)
    if ($browsers.Count -ne 1) { throw 'Birden fazla veya sıfır Sqywein var. -BrowserId ile süreç kimliğini seç.' }
    $BrowserId = $browsers[0].Id
}
$rootName = (Get-Process -Id $BrowserId).ProcessName
if ($rootName -notin @('sqywein','chrome','msedge','brave','opera','firefox')) { throw 'Seçilen süreç desteklenen bir tarayıcı değil.' }
function Get-BrowserTree {
    $all = @(Get-CimInstance Win32_Process)
    $ids = [System.Collections.Generic.HashSet[uint32]]::new()
    [void]$ids.Add([uint32]$BrowserId)
    do {
        $changed = $false
        foreach ($item in $all) { if ($ids.Contains($item.ParentProcessId) -and $ids.Add($item.ProcessId)) { $changed = $true } }
    } while ($changed)
    @($all | Where-Object { $ids.Contains($_.ProcessId) })
}
$before = @{}
foreach ($item in Get-BrowserTree) { $p = Get-Process -Id $item.ProcessId -ErrorAction SilentlyContinue; if ($p) { $before[$p.Id] = $p.CPU } }
$watch = [Diagnostics.Stopwatch]::StartNew()
Start-Sleep -Seconds ([Math]::Min(30, [Math]::Max(1, $Seconds)))
$tree = @(Get-BrowserTree)
$after = @{}
foreach ($item in $tree) { $p = Get-Process -Id $item.ProcessId -ErrorAction SilentlyContinue; if ($p) { $after[$p.Id] = $p.CPU } }
$elapsed = $watch.Elapsed.TotalSeconds
$counters = @(Get-CimInstance Win32_PerfFormattedData_PerfProc_Process)
$rows = @()
$totalCpu = 0.0
foreach ($item in $tree) {
    $counter = $counters | Where-Object IDProcess -eq $item.ProcessId | Select-Object -First 1
    $p = Get-Process -Id $item.ProcessId -ErrorAction SilentlyContinue
    $cpuDelta = 0.0
    if ($after.ContainsKey([int]$item.ProcessId)) {
        $startCpu = if ($before.ContainsKey([int]$item.ProcessId)) { $before[[int]$item.ProcessId] } else { 0 }
        $cpuDelta = [Math]::Max([double]0, [double]($after[[int]$item.ProcessId] - $startCpu))
        $totalCpu += $cpuDelta
    }
    if (!$p) { continue }
    if (!$counter) { throw "Bellek sayacı bulunamadı: $($item.ProcessId)" }
    $role = if ($item.Name -eq 'sqywein.exe') { 'Rust shell' } elseif ($item.CommandLine -match '--type=([^ ]+)') { $Matches[1] } else { 'Browser' }
    $rows += [pscustomobject]@{ Id = $item.ProcessId; Role = $role; PrivateWorkingSetMB = [math]::Round($counter.WorkingSetPrivate / 1MB, 1); CpuSeconds = [math]::Round($cpuDelta, 4) }
}
$shell = ($rows | Where-Object Role -eq 'Rust shell' | Measure-Object PrivateWorkingSetMB -Sum).Sum
$total = ($rows | Measure-Object PrivateWorkingSetMB -Sum).Sum
if (!($rows | Where-Object Id -eq $BrowserId)) { throw 'Tarayıcı ölçüm sırasında kapandı; örnek geçersiz.' }
[pscustomobject]@{
    BrowserId = $BrowserId
    Browser = $rootName
    Seconds = [math]::Round($elapsed, 2)
    ShellMB = $shell
    EngineMB = [math]::Round($total - $shell, 1)
    TotalMB = [math]::Round($total, 1)
    CpuPercent = [math]::Round(100 * $totalCpu / $elapsed / [Environment]::ProcessorCount, 2)
    CpuSeconds = [math]::Round($totalCpu, 4)
    Processes = $rows
} | ConvertTo-Json -Depth 4
