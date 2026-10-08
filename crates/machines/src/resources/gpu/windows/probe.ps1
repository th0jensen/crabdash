
$engines = @(); $memory = @(); $inventory = @(); $supported = $false; $inventory_available = $false
try {
    $class = Get-CimClass -ClassName Win32_PerfRawData_GPUPerformanceCounters_GPUEngine -ErrorAction Stop
    $type = $class.CimClassProperties['UtilizationPercentage'].Qualifiers['CounterType'].Value
    if ([uint64]$type -eq 542180608) {
        $engines = @(Get-CimInstance -ClassName Win32_PerfRawData_GPUPerformanceCounters_GPUEngine -ErrorAction Stop | ForEach-Object {
            if ($null -eq $_.UtilizationPercentage -or $null -eq $_.Timestamp_Sys100NS) { throw 'GPU engine counters unavailable' }
            [pscustomobject]@{ id = [string]$_.Name; ticks = ([uint64]$_.UtilizationPercentage).ToString(); clock = ([uint64]$_.Timestamp_Sys100NS).ToString() }
        })
        $supported = $true
    }
} catch { }
try {
    $memory = @(Get-CimInstance -ClassName Win32_PerfRawData_GPUPerformanceCounters_GPUAdapterMemory -ErrorAction Stop | ForEach-Object {
        if ($null -eq $_.DedicatedUsage) { throw 'GPU memory counters unavailable' }
        [pscustomobject]@{ id = [string]$_.Name; used = ([uint64]$_.DedicatedUsage).ToString() }
    })
} catch { }
$native_available = $false; $adapters = @()
if ($crabdash_gpu_discover) {
    try {
        $inventory = @(Get-CimInstance -ClassName Win32_VideoController -ErrorAction Stop | ForEach-Object {
            [pscustomobject]@{ id = [string]$_.PNPDeviceID; name = [string]$_.Name; vendor = [string]$_.AdapterCompatibility; driver = [string]$_.DriverVersion }
        })
        $inventory_available = $true
    } catch { }
    $native = $null
    try {
        Add-Type -TypeDefinition $crabdash_gpu_inventory -ErrorAction Stop
        $native = [CrabdashGpu.Probe]::Collect([string[]]@($inventory | ForEach-Object { $_.id }))
    } catch { }
    if ($null -ne $native) { $native_available = $native.available; $adapters = @($native.adapters) }
}
[pscustomobject]@{ native_available = $native_available; adapters = $adapters; supported = $supported; inventory_available = $inventory_available; engines = $engines; memory = $memory; inventory = $inventory } | ConvertTo-Json -Depth 4 -Compress
