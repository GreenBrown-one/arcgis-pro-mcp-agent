[CmdletBinding()]
param([string]$ArtifactRoot)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

function Assert-True {
    param([bool]$Actual, [string]$Message)
    if (-not $Actual) { throw "Assertion failed: $Message" }
}

function Assert-False {
    param([bool]$Actual, [string]$Message)
    if ($Actual) { throw "Assertion failed: $Message" }
}

function Assert-Equal {
    param($Actual, $Expected, [string]$Message)
    if ($Actual -ne $Expected) { throw "Assertion failed: $Message. Expected '$Expected', got '$Actual'." }
}

function Assert-NoTextMatch {
    param([string[]]$Paths, [string]$Pattern)
    $matches = @($Paths | ForEach-Object { Select-String -LiteralPath $_ -Pattern $Pattern })
    if ($matches.Count -gt 0) { throw "Unexpected text matching '$Pattern': $($matches.Path -join ', ')" }
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$PreviewConfig = Join-Path $repoRoot 'apps\desktop\src-tauri\tauri.preview.conf.json'
$BuildScript = Join-Path $repoRoot 'scripts\Build-Preview.ps1'
$HookScript = Join-Path $repoRoot 'apps\desktop\src-tauri\windows\hooks.nsh'

foreach ($path in @($PreviewConfig, $BuildScript, $HookScript)) {
    Assert-True (Test-Path -LiteralPath $path -PathType Leaf) "Required packaging source is missing: $path"
}

$config = Get-Content -LiteralPath $PreviewConfig -Raw -Encoding utf8 | ConvertFrom-Json
Assert-Equal $config.version '0.2.0-preview.1' 'preview version'
Assert-True $config.bundle.active 'bundle enabled'
Assert-True ($config.bundle.targets -contains 'nsis') 'NSIS target'
Assert-Equal $config.bundle.windows.nsis.installMode 'currentUser' 'per-user install'
Assert-True ($config.bundle.externalBin -contains 'generated/preview/ArcGISProAgent.Mcp') 'MCP sidecar'
Assert-False (($config.bundle.externalBin -join '|') -match '(?i)codex') 'Codex must stay external'
Assert-NoTextMatch -Paths @($PreviewConfig, $BuildScript) -Pattern 'CodexExe|WindowsApps|D:\\arcgis_pro'
Assert-NoTextMatch -Paths @($PreviewConfig, $BuildScript) -Pattern 'deepseek'

$hooks = Get-Content -LiteralPath $HookScript -Raw -Encoding utf8
Assert-True ($hooks -match '(?s)!macro NSIS_HOOK_POSTINSTALL.*?CreateShortCut "\$DESKTOP\\ArcGIS Pro \u667A\u80FD\u52A9\u624B\.lnk" "\$INSTDIR\\arcgis-pro-agent-desktop\.exe".*?!macroend') 'postinstall creates only the named desktop shortcut'
Assert-True ($hooks -match '(?s)!macro NSIS_HOOK_PREUNINSTALL.*?ExecWait ''"\$INSTDIR\\arcgis-pro-agent-desktop\.exe" --uninstall-cleanup'' \$0.*?StrCmp \$0 0 cleanup_succeeded.*?Abort.*?cleanup_succeeded:.*?Delete "\$DESKTOP\\ArcGIS Pro \u667A\u80FD\u52A9\u624B\.lnk".*?!macroend') 'preuninstall aborts before deleting the named shortcut when exact cleanup fails'
Assert-False ($hooks -match '(?i)(?:AddIns|esriAddInX|Delete\s+\$APPDATA|RMDir\s+/r)') 'hooks must not enumerate or remove AddIns or broad user data'

if (-not [string]::IsNullOrWhiteSpace($ArtifactRoot)) {
    $installerName = 'ArcGISPro' + [string][char]0x667A + [string][char]0x80FD + [string][char]0x52A9 + [string][char]0x624B + '-0.2.0-preview.1-x64-setup.exe'
    $installer = Join-Path $ArtifactRoot $installerName
    $hashFile = "$installer.sha256"
    Assert-True (Test-Path -LiteralPath $installer -PathType Leaf) 'required preview installer filename'
    Assert-True (Test-Path -LiteralPath $hashFile -PathType Leaf) 'required lowercase SHA-256 file'
    $hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    Assert-Equal (Get-Content -LiteralPath $hashFile -Raw -Encoding utf8).TrimEnd() "$hash  $installerName" 'recorded lowercase SHA-256'
}

Write-Host 'Preview packaging assertions passed.' -ForegroundColor Green
