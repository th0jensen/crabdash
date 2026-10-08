# Discovery only: never invoke Docker or probe its daemon.
function Resolve-CrabdashDocker {
    param([string]$Cached, [string[]]$Paths, [string[]]$Defaults)
    $candidates = @($Cached)
    foreach ($path in $Paths) {
        if (-not $path) { continue }
        foreach ($directory in $path.Split(';')) {
            $directory = [Environment]::ExpandEnvironmentVariables($directory.Trim().Trim('"'))
            if ($directory) { $candidates += $directory.TrimEnd('\', '/') + '/docker.exe' }
        }
    }
    $candidates += $Defaults
    foreach ($candidate in $candidates) {
        if (-not $candidate) { continue }
        try {
            $candidate = [Environment]::ExpandEnvironmentVariables($candidate.Trim().Trim('"'))
            if ([IO.Path]::GetExtension($candidate) -ieq '.exe' -and
                (Test-Path -LiteralPath $candidate -PathType Leaf -ErrorAction SilentlyContinue)) {
                return (Get-Item -LiteralPath $candidate -ErrorAction Stop).FullName
            }
        } catch { } # A malformed/stale entry must not conceal the remaining CLI candidates.
    }
    $command = Get-Command -Name 'docker.exe' -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($command -and [IO.Path]::GetExtension($command.Source) -ieq '.exe' -and
        (Test-Path -LiteralPath $command.Source -PathType Leaf -ErrorAction SilentlyContinue)) {
        return $command.Source
    }
}
