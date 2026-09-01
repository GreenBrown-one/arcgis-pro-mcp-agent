[CmdletBinding()]
param(
    [string]$ArcGISProInstallDir,
    [string]$InstallRoot = (Join-Path $env:LOCALAPPDATA 'ArcGISProAgent\dev'),
    [string]$AddInRoot
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

$version = '0.1.0'
$repoRoot = [IO.Path]::GetFullPath((Split-Path -Parent $PSScriptRoot)).TrimEnd('\')
$resolver = Join-Path $repoRoot 'scripts\Resolve-ArcGISProInstall.ps1'
$coreModule = Join-Path $repoRoot 'scripts\Install-Dev.Core.psm1'
foreach ($required in @($resolver, $coreModule)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Required installation component is missing: $required"
    }
}
Import-Module $coreModule -Force

if ([string]::IsNullOrWhiteSpace($AddInRoot)) {
    $AddInRoot = Get-ArcGISProAgentDefaultAddInRoot
}

$topology = Test-ArcGISProAgentInstallTopology -SourceRoot $repoRoot `
    -InstallRoot $InstallRoot -AddInRoot $AddInRoot
$repoRoot = $topology.SourceRoot
$InstallRoot = $topology.InstallRoot
$AddInRoot = $topology.AddInRoot

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

if ([string]::IsNullOrWhiteSpace($ArcGISProInstallDir)) {
    $ArcGISProInstallDir = & $resolver -Candidate 'D:\arcgis_pro'
}
$ArcGISProInstallDir = [IO.Path]::GetFullPath($ArcGISProInstallDir).TrimEnd('\')
if (-not (Test-Path -LiteralPath (Join-Path $ArcGISProInstallDir 'bin\ArcGIS.Core.dll') -PathType Leaf)) {
    throw "ArcGIS Pro SDK assemblies were not found below: $ArcGISProInstallDir"
}

$mcpProject = Join-Path $repoRoot 'src\ArcGISProAgent.Mcp\ArcGISProAgent.Mcp.csproj'
$addInProject = Join-Path $repoRoot 'src\ArcGISProAgent.AddIn\ArcGISProAgent.AddIn.csproj'
$desktopRoot = Join-Path $repoRoot 'apps\desktop'
$noRegisterFolder = Join-Path ([IO.Path]::GetTempPath()) ('ArcGISProAgent-no-register-' + [Guid]::NewGuid().ToString('N'))
if (Test-Path -LiteralPath $noRegisterFolder) {
    throw "The Add-In no-register guard path must not exist: $noRegisterFolder"
}

Invoke-CheckedCommand -Command 'dotnet' -Arguments @(
    'build', $mcpProject, '--configuration', 'Release'
) -FailureMessage 'MCP Release build failed'
Invoke-CheckedCommand -Command 'dotnet' -Arguments @(
    'build', $addInProject, '--configuration', 'Release',
    "-p:ArcGISProInstallDir=$ArcGISProInstallDir", "-p:ArcGISFolder=$noRegisterFolder"
) -FailureMessage 'ArcGIS Pro Add-In Release build failed'

Push-Location $desktopRoot
try {
    Invoke-CheckedCommand -Command 'npm.cmd' -Arguments @(
        'run', 'tauri', '--', 'build', '--debug', '--no-bundle'
    ) -FailureMessage 'Tauri debug build failed'
} finally {
    Pop-Location
}

$mcpOutput = Join-Path $repoRoot 'src\ArcGISProAgent.Mcp\bin\Release\net8.0'
$addInPackage = Join-Path $repoRoot 'src\ArcGISProAgent.AddIn\bin\Release\net10.0-windows\ArcGISProAgent.AddIn.esriAddinX'
$desktopExe = Join-Path $repoRoot 'apps\desktop\src-tauri\target\debug\arcgis-pro-agent-desktop.exe'
foreach ($requiredOutput in @($mcpOutput, $addInPackage, $desktopExe)) {
    if (-not (Test-Path -LiteralPath $requiredOutput)) {
        throw "Expected build output is missing: $requiredOutput"
    }
}

$records = New-Object System.Collections.Generic.List[object]
foreach ($sourceFile in @(Get-ChildItem -LiteralPath $mcpOutput -Recurse -File)) {
    $relative = $sourceFile.FullName.Substring($mcpOutput.Length).TrimStart('\')
    $records.Add([PSCustomObject]@{
        SourcePath = $sourceFile.FullName
        DestinationPath = (Join-Path $InstallRoot "$version\mcp\$relative")
        Component = 'mcp'
    })
}
$records.Add([PSCustomObject]@{
    SourcePath = $desktopExe
    DestinationPath = (Join-Path $InstallRoot "$version\desktop\arcgis-pro-agent-desktop.exe")
    Component = 'desktop'
})
$records.Add([PSCustomObject]@{
    SourcePath = $addInPackage
    DestinationPath = (Join-Path $AddInRoot 'ArcGISProAgent.AddIn.esriAddinX')
    Component = 'addin'
})

$result = Invoke-ArcGISProAgentManifestInstallWithDefaultAddInMigration -SourceRoot $repoRoot `
    -InstallRoot $InstallRoot -AddInRoot $AddInRoot `
    -Version $version -Files @($records | ForEach-Object { $_ })

$localRoot = Join-Path $env:LOCALAPPDATA 'ArcGISProAgent'
Write-Host "Source:   $repoRoot"
Write-Host "Install:  $InstallRoot"
Write-Host "Add-In:   $AddInRoot"
Write-Host "Manifest: $($result.ManifestPath)"
Write-Host "Config:   $(Join-Path $localRoot 'config')"
Write-Host "Logs:     $(Join-Path $localRoot 'logs')"
Write-Host "Runtime:  $(Join-Path $localRoot 'runtime')"
Write-Host "Installed $($result.FileCount) manifest-owned files for version $version." -ForegroundColor Green
