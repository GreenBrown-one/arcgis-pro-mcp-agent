[CmdletBinding()]
param(
    [string]$ArcGISProInstallDir,
    [switch]$SkipTests
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

function Invoke-Checked {
    param([Parameter(Mandatory = $true)][scriptblock]$Command, [Parameter(Mandatory = $true)][string]$FailureMessage)
    & $Command
    if ($LASTEXITCODE -ne 0) { throw $FailureMessage }
}

function Require-ExactlyOne {
    param([Parameter(Mandatory = $true)][object[]]$Items, [Parameter(Mandatory = $true)][string]$Description)
    if ($Items.Count -ne 1) { throw "Expected exactly one $Description, found $($Items.Count)." }
    return $Items[0]
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$desktopRoot = Join-Path $repoRoot 'apps\desktop'
$tauriRoot = Join-Path $desktopRoot 'src-tauri'
$stagingRoot = Join-Path $tauriRoot 'generated\preview'
$mcpProject = Join-Path $repoRoot 'src\ArcGISProAgent.Mcp\ArcGISProAgent.Mcp.csproj'
$addInProject = Join-Path $repoRoot 'src\ArcGISProAgent.AddIn\ArcGISProAgent.AddIn.csproj'
$mcpPublish = Join-Path $stagingRoot 'mcp-publish'
$artifacts = Join-Path $repoRoot 'artifacts\preview'
$installerName = 'ArcGISPro' + [string][char]0x667A + [string][char]0x80FD + [string][char]0x52A9 + [string][char]0x624B + '-0.2.0-preview.1-x64-setup.exe'
$installer = Join-Path $artifacts $installerName
$hashFile = "$installer.sha256"

$arcgis = & (Join-Path $PSScriptRoot 'Resolve-ArcGISProInstall.ps1') -Candidate $ArcGISProInstallDir
if (-not $SkipTests) {
    & (Join-Path $PSScriptRoot 'Test-Foundation.ps1') -ArcGISProInstallDir $arcgis
    if ($LASTEXITCODE -ne 0) { throw 'Foundation verification failed.' }
}

if (Test-Path -LiteralPath $stagingRoot) { Remove-Item -LiteralPath $stagingRoot -Recurse -Force }
New-Item -ItemType Directory -Path $stagingRoot -Force | Out-Null

Invoke-Checked -FailureMessage 'MCP publish failed.' -Command {
    dotnet publish $mcpProject -c Release -r win-x64 --self-contained true `
        -p:PublishSingleFile=true -p:PublishTrimmed=false -p:Version=0.2.0-preview.1 `
        -o $mcpPublish
}

Invoke-Checked -FailureMessage 'Add-In build failed.' -Command {
    dotnet build $addInProject -c Release -p:ArcGISProInstallDir=$arcgis `
        -p:Version=0.2.0-preview.1
}

$mcpExecutable = Require-ExactlyOne -Items @(Get-ChildItem -LiteralPath $mcpPublish -Filter '*.exe' -File) -Description 'MCP executable'
$addInPackage = Require-ExactlyOne -Items @(Get-ChildItem -LiteralPath (Split-Path -Parent $addInProject) -Recurse -Filter '*.esriAddInX' -File |
    Where-Object { $_.FullName -match '[\\/]bin[\\/]Release[\\/]' }) -Description 'Add-In package'

Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead($addInPackage.FullName)
try {
    $arcgisAssemblies = @($archive.Entries | Where-Object { $_.FullName -match '(^|/)ArcGIS\..*\.dll$' })
    if ($arcgisAssemblies.Count -ne 0) {
        throw "Add-In package redistributes ArcGIS assemblies: $($arcgisAssemblies.FullName -join ', ')"
    }
} finally {
    $archive.Dispose()
}

Copy-Item -LiteralPath $mcpExecutable.FullName -Destination (Join-Path $stagingRoot 'ArcGISProAgent.Mcp-x86_64-pc-windows-msvc.exe')
Copy-Item -LiteralPath $addInPackage.FullName -Destination (Join-Path $stagingRoot 'ArcGISProAgent.AddIn.esriAddInX')

Push-Location $desktopRoot
try {
    Invoke-Checked -FailureMessage 'npm ci failed.' -Command { npm.cmd ci }
    Invoke-Checked -FailureMessage 'Tauri NSIS build failed.' -Command {
        npm.cmd run tauri -- build --config src-tauri/tauri.preview.conf.json `
            --bundles nsis --target x86_64-pc-windows-msvc
    }
} finally {
    Pop-Location
}

$nsisInstaller = Require-ExactlyOne -Items @(Get-ChildItem -LiteralPath (Join-Path $tauriRoot 'target\x86_64-pc-windows-msvc\release\bundle\nsis') -Filter '*.exe' -File) -Description 'NSIS installer'
if (Test-Path -LiteralPath $artifacts) { Remove-Item -LiteralPath $artifacts -Recurse -Force }
New-Item -ItemType Directory -Path $artifacts -Force | Out-Null
Copy-Item -LiteralPath $nsisInstaller.FullName -Destination $installer -Force
$hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText($hashFile, "$hash  $installerName`r`n", [Text.UTF8Encoding]::new($false))

Write-Host "Preview installer: $installer"
Write-Host "SHA-256: $hash"
