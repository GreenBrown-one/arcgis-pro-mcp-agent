[CmdletBinding()]
param(
    [string]$ArcGISProInstallDir
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

function Invoke-CheckedCommand {
    param(
        [Parameter(Mandatory = $true)][string]$Command,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$FailureMessage
    )

    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$FailureMessage (exit code $LASTEXITCODE)"
    }
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$arcgis = & (Join-Path $PSScriptRoot 'Resolve-ArcGISProInstall.ps1') -Candidate $ArcGISProInstallDir
if ($LASTEXITCODE -ne 0) {
    throw "ArcGIS Pro installation resolution failed (exit code $LASTEXITCODE)"
}

Invoke-CheckedCommand -Command 'powershell' -Arguments @(
    '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
    (Join-Path $PSScriptRoot 'Test-Foundation.ps1'), '-ArcGISProInstallDir', $arcgis
) -FailureMessage 'Foundation verification failed'

Push-Location $repoRoot
try {
    Invoke-CheckedCommand -Command 'dotnet' -Arguments @('test', 'McpServer.sln', '--no-restore') `
        -FailureMessage '.NET preview verification failed'

    Push-Location 'apps\desktop'
    try {
        Invoke-CheckedCommand -Command 'npm' -Arguments @('test') -FailureMessage 'Frontend tests failed'
        Invoke-CheckedCommand -Command 'npm' -Arguments @('run', 'build') -FailureMessage 'Frontend build failed'

        Push-Location 'src-tauri'
        try {
            Invoke-CheckedCommand -Command 'cargo' -Arguments @('test') -FailureMessage 'Rust tests failed'
            Invoke-CheckedCommand -Command 'cargo' -Arguments @('check', '--release') -FailureMessage 'Rust release check failed'
        } finally {
            Pop-Location
        }
    } finally {
        Pop-Location
    }

    Invoke-CheckedCommand -Command 'powershell' -Arguments @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
        (Join-Path $PSScriptRoot 'Test-PreviewPackaging.ps1')
    ) -FailureMessage 'Preview packaging assertions failed'

    Invoke-CheckedCommand -Command 'git' -Arguments @('-c', "safe.directory=$repoRoot", '-C', $repoRoot, 'diff', '--check') `
        -FailureMessage 'Git whitespace check failed'
} finally {
    Pop-Location
}

Write-Host 'Preview automated verification passed.' -ForegroundColor Green
