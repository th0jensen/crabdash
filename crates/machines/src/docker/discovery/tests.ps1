# Runs after windows.ps1, either through the Windows Rust test or a standalone QA shell.
$ErrorActionPreference = 'Stop'
function Assert-Path([string]$Actual, [string]$Expected, [string]$Scenario) {
    if ($Actual -ne $Expected) { throw "$Scenario : expected <$Expected>, got <$Actual>" }
}
$root = Join-Path ([IO.Path]::GetTempPath()) ('crabdash-docker-discovery-' + [Guid]::NewGuid().ToString('N'))
$originalPath = $env:PATH
$originalRoot = $env:CRABDASH_DOCKER_ROOT
$originalProgramFiles = $env:ProgramFiles
$originalLocalAppData = $env:LOCALAPPDATA
try {
    $env:PATH = '' # No installed client may satisfy a fixture's missing-CLI case.
    # The dollar expression in the directory name is literal data.
    $directory = Join-Path $root 'user''s ‘folder’ [1] $(literal)'
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    $client = Join-Path $directory 'docker.exe'
    [IO.File]::WriteAllText($client, 'fixture executable; discovery must not run this file')
    $env:CRABDASH_DOCKER_ROOT = $directory
    Assert-Path (Resolve-CrabdashDocker -Cached $client) $client 'Cached path with literal shell characters'
    Assert-Path (Resolve-CrabdashDocker -Cached (Join-Path $root 'missing.exe') -Paths ('"%CRABDASH_DOCKER_ROOT%"')) $client 'Expanded and quoted PATH'
    Assert-Path (Resolve-CrabdashDocker -Cached '%CRABDASH_DOCKER_ROOT%/docker.exe') $client 'Expanded cached path'
    $invalid = Join-Path $root 'docker.cmd'
    [IO.File]::WriteAllText($invalid, '@echo injection')
    foreach ($name in @('docker', 'docker.cmd', 'docker.ps1', 'docker.exe.txt')) {
        $notExecutable = Join-Path $root $name
        [IO.File]::WriteAllText($notExecutable, 'fixture script')
        Assert-Path (Resolve-CrabdashDocker -Cached $notExecutable -Defaults $client) $client "Reject non-executable extension $name"
    }
    $upper = Join-Path $root 'DOCKER.EXE'
    [IO.File]::WriteAllText($upper, 'fixture executable')
    Assert-Path (Resolve-CrabdashDocker -Cached $upper) $upper 'Executable extension is case insensitive'
    $directoryExe = Join-Path $root 'directory.exe'
    New-Item -ItemType Directory -Path $directoryExe | Out-Null
    Assert-Path (Resolve-CrabdashDocker -Cached $directoryExe -Defaults $client) $client 'Reject directories'
    Assert-Path (Resolve-CrabdashDocker -Cached ("invalid" + [char]0 + '.exe') -Defaults $client) $client 'Malformed candidate does not hide a valid client'
    $second = Join-Path $root 'different docker.exe'
    [IO.File]::WriteAllText($second, 'fixture')
    Assert-Path (Resolve-CrabdashDocker -Cached $client -Paths $directory -Defaults $second) $client 'Cached CLI keeps precedence'
    Assert-Path (Resolve-CrabdashDocker -Paths $directory -Defaults $second) $client 'Configured PATH keeps precedence'
    $env:ProgramFiles = Join-Path $root 'Program Files'
    $allUsers = Join-Path $env:ProgramFiles 'Docker/Docker/resources/bin/docker.exe'
    New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($allUsers)) -Force | Out-Null
    [IO.File]::WriteAllText($allUsers, 'fixture')
    Assert-Path (Resolve-CrabdashDocker -Defaults '%ProgramFiles%/Docker/Docker/resources/bin/docker.exe') $allUsers 'All-users Desktop install without PATH'
    $env:LOCALAPPDATA = Join-Path $root 'Local App Data'
    $perUser = Join-Path $env:LOCALAPPDATA 'Programs/DockerDesktop/resources/bin/docker.exe'
    New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($perUser)) -Force | Out-Null
    [IO.File]::WriteAllText($perUser, 'fixture')
    Assert-Path (Resolve-CrabdashDocker -Defaults '%LOCALAPPDATA%/Programs/DockerDesktop/resources/bin/docker.exe') $perUser 'Per-user Desktop install without PATH'
    Assert-Path (Resolve-CrabdashDocker -Cached $invalid -Paths (Join-Path $root 'missing') -Defaults (Join-Path $root 'missing.exe')) '' 'Missing CLI is empty discovery'
} finally {
    $env:PATH = $originalPath
    $env:CRABDASH_DOCKER_ROOT = $originalRoot
    $env:ProgramFiles = $originalProgramFiles
    $env:LOCALAPPDATA = $originalLocalAppData
    Remove-Item -LiteralPath $root -Force -Recurse -ErrorAction SilentlyContinue
}
