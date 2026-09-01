[CmdletBinding()]
param(
    [string]$WorkspaceRoot,
    [string]$TestNamePattern = '.*'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

if ([string]::IsNullOrWhiteSpace($WorkspaceRoot)) {
    $WorkspaceRoot = Split-Path -Parent $PSScriptRoot
}
$repoRoot = [IO.Path]::GetFullPath($WorkspaceRoot).TrimEnd('\')
$modulePath = Join-Path $repoRoot 'scripts\Install-Dev.Core.psm1'
$installerCoreModule = Import-Module $modulePath -Force -PassThru

$script:passed = 0
$script:junctions = New-Object System.Collections.Generic.List[string]
$script:substDrive = $null
$script:childProcesses = New-Object System.Collections.Generic.List[object]
$script:externalTempPaths = New-Object System.Collections.Generic.List[string]
$script:otherTempPaths = New-Object System.Collections.Generic.List[string]
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
$testRoot = Join-Path $tempRoot ("ArcGISProAgent-InstallerTests-" + [Guid]::NewGuid().ToString('N'))
if (-not $testRoot.StartsWith($tempRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw "Unsafe installer test root: $testRoot"
}
if (Test-Path -LiteralPath $testRoot) {
    throw "Installer test root already exists: $testRoot"
}
New-Item -ItemType Directory -Path $testRoot | Out-Null

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw "Assertion failed: $Message" }
}

function Assert-Equal {
    param($Actual, $Expected, [string]$Message)
    if ($Actual -ne $Expected) {
        throw "Assertion failed: $Message. Expected '$Expected', got '$Actual'."
    }
}

function Assert-Throws {
    param([scriptblock]$Action, [string]$Pattern, [string]$Message)
    $threw = $false
    try {
        & $Action
    } catch {
        $threw = $true
        if (-not [string]::IsNullOrWhiteSpace($Pattern) -and $_.Exception.Message -notmatch $Pattern) {
            throw "Assertion failed: $Message. Wrong error: $($_.Exception.Message)"
        }
    }
    if (-not $threw) { throw "Assertion failed: $Message. Expected an exception." }
}

function Invoke-Test {
    param([string]$Name, [scriptblock]$Body)
    if ($Name -notmatch $TestNamePattern) { return }
    & $Body
    $script:passed++
    Write-Host "PASS $Name" -ForegroundColor Green
}

function New-TestCase {
    param([string]$Name)
    $base = Join-Path $testRoot $Name
    $source = Join-Path $base 'artifacts'
    New-Item -ItemType Directory -Force -Path $source | Out-Null
    [PSCustomObject]@{
        Base = $base
        Source = $source
        InstallRoot = (Join-Path $base 'install')
        AddInRoot = (Join-Path $base 'addin')
        Manifest = (Join-Path $base 'install\install-manifest.json')
    }
}

function Set-TestArtifacts {
    param($Case, [string]$Content)
    [IO.File]::WriteAllText((Join-Path $Case.Source 'Agent.dll'), "mcp-$Content")
    [IO.File]::WriteAllText((Join-Path $Case.Source 'agent.exe'), "desktop-$Content")
    [IO.File]::WriteAllText((Join-Path $Case.Source 'ArcGISProAgent.AddIn.esriAddinX'), "addin-$Content")
}

function Get-TestRecords {
    param($Case, [switch]$WithoutDesktop)
    $records = @(
        [PSCustomObject]@{
            SourcePath = (Join-Path $Case.Source 'Agent.dll')
            DestinationPath = (Join-Path $Case.InstallRoot '0.1.0\mcp\Agent.dll')
            Component = 'mcp'
        }
    )
    if (-not $WithoutDesktop) {
        $records += [PSCustomObject]@{
            SourcePath = (Join-Path $Case.Source 'agent.exe')
            DestinationPath = (Join-Path $Case.InstallRoot '0.1.0\desktop\agent.exe')
            Component = 'desktop'
        }
    }
    $records += [PSCustomObject]@{
        SourcePath = (Join-Path $Case.Source 'ArcGISProAgent.AddIn.esriAddinX')
        DestinationPath = (Join-Path $Case.AddInRoot 'ArcGISProAgent.AddIn.esriAddinX')
        Component = 'addin'
    }
    $records
}

function New-LegacyInstallFixture {
    param($Case, [string]$LegacyAddInRoot, [string]$Content)
    $mcp = Join-Path $Case.InstallRoot '0.1.0\mcp\Agent.dll'
    $desktop = Join-Path $Case.InstallRoot '0.1.0\desktop\agent.exe'
    $addin = Join-Path $LegacyAddInRoot '0.1.0\ArcGISProAgent.AddIn.esriAddinX'
    foreach ($path in @($mcp, $desktop, $addin)) {
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
    }
    [IO.File]::WriteAllText($mcp, "mcp-$Content")
    [IO.File]::WriteAllText($desktop, "desktop-$Content")
    [IO.File]::WriteAllText($addin, "addin-$Content")
    $entries = foreach ($item in @(
        [PSCustomObject]@{ Path=$mcp; Component='mcp' },
        [PSCustomObject]@{ Path=$desktop; Component='desktop' },
        [PSCustomObject]@{ Path=$addin; Component='addin' }
    )) {
        $file = Get-Item -LiteralPath $item.Path
        [ordered]@{
            owner='ArcGISProAgent'; version='0.1.0'; component=$item.Component
            path=[IO.Path]::GetFullPath($item.Path); sha256=(Get-FileHash -LiteralPath $item.Path -Algorithm SHA256).Hash.ToLowerInvariant()
            length=[long]$file.Length
        }
    }
    New-Item -ItemType Directory -Force -Path $Case.InstallRoot | Out-Null
    $manifest = [ordered]@{
        schemaVersion=1; owner='ArcGISProAgent'; version='0.1.0'; generatedAtUtc=[DateTime]::UtcNow.ToString('o')
        manifestPath=[IO.Path]::GetFullPath($Case.Manifest); installRoot=[IO.Path]::GetFullPath($Case.InstallRoot).TrimEnd('\')
        addInRoot=[IO.Path]::GetFullPath($LegacyAddInRoot).TrimEnd('\'); files=@($entries)
    }
    [IO.File]::WriteAllText($Case.Manifest, ($manifest | ConvertTo-Json -Depth 7), (New-Object Text.UTF8Encoding($false)))
    [PSCustomObject]@{ Mcp=$mcp; Desktop=$desktop; AddIn=$addin }
}

function Assert-MigrationConverged {
    param($Case, $Legacy, [string]$StageRoot, [string]$Context)
    $records = @(Get-TestRecords $Case)
    $manifest = Get-Content -LiteralPath $Case.Manifest -Raw | ConvertFrom-Json
    $expectedPaths = @($records | ForEach-Object {
        [IO.Path]::GetFullPath([string]$_.DestinationPath)
    } | Sort-Object)
    $actualPaths = @($manifest.files | ForEach-Object { [string]$_.path } | Sort-Object)
    Assert-Equal $manifest.addInRoot ([IO.Path]::GetFullPath($Case.AddInRoot).TrimEnd('\')) `
        "$Context manifest uses the official root"
    Assert-Equal $actualPaths.Count 3 "$Context manifest owns exactly three files"
    Assert-Equal ($actualPaths -join '|') ($expectedPaths -join '|') `
        "$Context manifest owns exact MCP, desktop, and Add-In paths"
    Assert-True (-not (Test-Path -LiteralPath $Legacy.AddIn)) "$Context legacy Add-In is absent"
    Assert-Equal (Get-Text $records[0].DestinationPath) 'mcp-v2' "$Context MCP converged"
    Assert-Equal (Get-Text $records[1].DestinationPath) 'desktop-v2' "$Context desktop converged"
    Assert-Equal (Get-Text $records[2].DestinationPath) 'addin-v2' "$Context Add-In converged"
    Assert-True (-not (Test-Path -LiteralPath $StageRoot)) "$Context TEMP stage is removed"
    Assert-NoTransactionArtifacts $Case
}

function Invoke-TestInstall {
    param(
        $Case,
        [object[]]$Records = (Get-TestRecords $Case),
        [string]$SourceRoot = $repoRoot,
        [string]$FailurePoint = 'None'
    )
    Invoke-ArcGISProAgentManifestInstall -SourceRoot $SourceRoot `
        -InstallRoot $Case.InstallRoot -AddInRoot $Case.AddInRoot `
        -Version '0.1.0' -Files $Records -FailurePoint $FailurePoint | Out-Null
}

function Get-Text {
    param([string]$Path)
    [IO.File]::ReadAllText($Path)
}

function Assert-NoTransactionArtifacts {
    param($Case)
    $leftovers = @(Get-ChildItem -LiteralPath $Case.Base -Recurse -Force -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^\.arcgis-pro-agent-(?:install-journal|journal-write|new|backup|rollback|manifest)' })
    Assert-Equal $leftovers.Count 0 'transaction artifacts must be cleaned'
}

function New-TestJunction {
    param([string]$Path, [string]$Target)
    New-Item -ItemType Directory -Force -Path $Target | Out-Null
    New-Item -ItemType Junction -Path $Path -Target $Target | Out-Null
    $script:junctions.Add([IO.Path]::GetFullPath($Path))
}

function Get-AvailableSubstDrive {
    foreach ($letter in @('Z','Y','X','W','V','U','T','S','R')) {
        $drive = "${letter}:"
        if (-not (Test-Path -LiteralPath ($drive + '\'))) { return $drive }
    }
    throw 'No unused drive letter is available for the SUBST safety test.'
}

function Set-TestSubst {
    param([string]$Target)
    if ($null -ne $script:substDrive) {
        & subst.exe $script:substDrive /d | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "Unable to remove prior test SUBST drive: $script:substDrive" }
        $script:substDrive = $null
    }
    $drive = Get-AvailableSubstDrive
    & subst.exe $drive $Target | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Unable to create test SUBST drive $drive for $Target" }
    $script:substDrive = $drive
    $drive
}

function Get-ShortPathIfAvailable {
    param([string]$Path)
    $command = "for %I in (`"$Path`") do @echo %~sI"
    $lines = @(& cmd.exe /d /c $command)
    if ($LASTEXITCODE -ne 0 -or $lines.Count -eq 0) { return $null }
    ([string]$lines[-1]).Trim()
}

function Assert-NoTestReparseAncestor {
    param([string]$Path)
    $full = [IO.Path]::GetFullPath($Path)
    $root = [IO.Path]::GetPathRoot($full)
    $current = $root
    foreach ($segment in @($full.Substring($root.Length).Split(@('\'), [StringSplitOptions]::RemoveEmptyEntries))) {
        $current = Join-Path $current $segment
        if (Test-Path -LiteralPath $current) {
            $item = Get-Item -LiteralPath $current -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Test cleanup refuses a reparse point: $current"
            }
        }
    }
}

function Assert-ExactTestStagePath {
    param([string]$StageRoot, [string]$TransactionId)
    if ($TransactionId -cnotmatch '^[0-9a-f]{32}$' -or
        [Guid]::ParseExact($TransactionId, 'N').ToString('N') -cne $TransactionId) {
        throw "Test journal transactionId is not normalized: $TransactionId"
    }
    $full = [IO.Path]::GetFullPath($StageRoot).TrimEnd('\')
    $expectedName = "ArcGISProAgent-install-$TransactionId"
    if ([IO.Path]::GetFileName($full) -cne $expectedName) {
        throw "Test journal stage name is not exact: $full"
    }
    $entityParent = [ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath((Split-Path -Parent $full))
    $tempEntity = [ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath($tempRoot)
    if (-not $entityParent.Equals($tempEntity, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Test journal stage parent is not the TEMP entity: $full"
    }
    Assert-NoTestReparseAncestor $full
    $full
}

function Register-CaseStageFromJournal {
    param($Case, [string]$JournalAddInRoot)
    if ([string]::IsNullOrWhiteSpace($JournalAddInRoot)) { $JournalAddInRoot = $Case.AddInRoot }
    $journalPath = Join-Path $Case.InstallRoot '.arcgis-pro-agent-install-journal.json'
    $beforeIdentity = [ArcGISProAgentInstaller.WindowsFileSystem]::GetFileIdentity($journalPath)
    $beforeHash = (Get-FileHash -LiteralPath $journalPath -Algorithm SHA256).Hash
    $journal = & $installerCoreModule {
        param($Path, $SourceRoot, $InstallRoot, $AddInRoot)
        Read-StrictInstallerJournal -JournalPath $Path -SourceRoot $SourceRoot `
            -InstallRoot $InstallRoot -AddInRoot $AddInRoot -Version '0.1.0'
    } $journalPath $repoRoot $Case.InstallRoot $JournalAddInRoot
    $afterIdentity = [ArcGISProAgentInstaller.WindowsFileSystem]::GetFileIdentity($journalPath)
    $afterHash = (Get-FileHash -LiteralPath $journalPath -Algorithm SHA256).Hash
    if ($beforeIdentity -ne $afterIdentity -or $beforeHash -ne $afterHash) {
        throw 'Test journal changed while deriving its stage path.'
    }
    $stage = Assert-ExactTestStagePath -StageRoot ([string]$journal.stageRoot) -TransactionId ([string]$journal.transactionId)
    if (-not $script:externalTempPaths.Contains($stage)) { $script:externalTempPaths.Add($stage) }
    $journal
}

function Remove-ExactTestStageTree {
    param([string]$StageRoot)
    $full = [IO.Path]::GetFullPath($StageRoot).TrimEnd('\')
    $name = [IO.Path]::GetFileName($full)
    if ($name -cnotmatch '^ArcGISProAgent-install-[0-9a-f]{32}$') { throw "Unsafe test stage name: $full" }
    $transactionId = $name.Substring('ArcGISProAgent-install-'.Length)
    Assert-ExactTestStagePath -StageRoot $full -TransactionId $transactionId | Out-Null
    if (-not (Test-Path -LiteralPath $full)) { return }
    $directories = New-Object System.Collections.Generic.List[string]
    $pending = New-Object System.Collections.Generic.Stack[string]
    $pending.Push($full)
    while ($pending.Count -gt 0) {
        $directory = $pending.Pop()
        Assert-NoTestReparseAncestor $directory
        $directories.Add($directory)
        foreach ($child in @(Get-ChildItem -LiteralPath $directory -Force)) {
            if (($child.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Test cleanup refuses a staged reparse point: $($child.FullName)"
            }
            if ($child.PSIsContainer) { $pending.Push($child.FullName) }
            else { Assert-NoTestReparseAncestor $child.FullName; [IO.File]::Delete($child.FullName) }
        }
    }
    foreach ($directory in @($directories | Sort-Object { $_.Length } -Descending)) {
        Assert-NoTestReparseAncestor $directory
        [IO.Directory]::Delete($directory, $false)
    }
}

function New-InstallerChildScript {
    param($Case)
    $path = Join-Path $Case.Base 'installer-child.ps1'
    $content = @'
[CmdletBinding()]
param(
    [string]$ModulePath,
    [string]$SourceRoot,
    [string]$InstallRoot,
    [string]$AddInRoot,
    [string]$ReadyPath,
    [string]$ReleasePath,
    [string]$Mode,
    [string]$FailurePoint = 'None'
)
$ErrorActionPreference = 'Stop'
Import-Module $ModulePath -Force
$script:migrationReplaceCount = 0
$source = Join-Path (Split-Path -Parent $ReadyPath) 'artifacts'
$records = @(
    [PSCustomObject]@{ SourcePath = (Join-Path $source 'Agent.dll'); DestinationPath = (Join-Path $InstallRoot '0.1.0\mcp\Agent.dll'); Component = 'mcp' },
    [PSCustomObject]@{ SourcePath = (Join-Path $source 'agent.exe'); DestinationPath = (Join-Path $InstallRoot '0.1.0\desktop\agent.exe'); Component = 'desktop' },
    [PSCustomObject]@{ SourcePath = (Join-Path $source 'ArcGISProAgent.AddIn.esriAddinX'); DestinationPath = (Join-Path $AddInRoot 'ArcGISProAgent.AddIn.esriAddinX'); Component = 'addin' }
)
if ($Mode -eq 'crash-before-backup-state-stale') {
    $records = @($records | Where-Object { $_.Component -ne 'desktop' })
}
$hook = {
    param([string]$EventName, $Context)
    if ($Mode -eq 'hold-lock' -and $EventName -eq 'LockAcquired') {
        [IO.File]::WriteAllText($ReadyPath, 'ready')
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        while (-not (Test-Path -LiteralPath $ReleasePath) -and [DateTime]::UtcNow -lt $deadline) {
            Start-Sleep -Milliseconds 50
        }
        if (-not (Test-Path -LiteralPath $ReleasePath)) { throw 'Timed out waiting for the lock-test release file.' }
    }
    if ($Mode -eq 'crash-after-replace' -and $EventName -eq 'AfterFirstReplace') {
        [Environment]::Exit(91)
    }
    if ($Mode -eq 'crash-before-backup-state-replace' -and $EventName -eq 'AfterFirstReplace') {
        [Environment]::Exit(91)
    }
    if ($Mode -eq 'crash-before-backup-state-manifest' -and $EventName -eq 'AfterManifestReplaceBeforeBackupState') {
        [Environment]::Exit(92)
    }
    if ($Mode -eq 'crash-before-backup-state-stale' -and $EventName -eq 'AfterStaleMoveBeforeBackupState') {
        [Environment]::Exit(93)
    }
    if ($Mode -like 'migration-hard-exit-*' -and $EventName -eq 'AfterFirstReplace') {
        $script:migrationReplaceCount++
        if ($Mode -eq 'migration-hard-exit-legacy' -and $script:migrationReplaceCount -eq 1) {
            [Environment]::Exit(94)
        }
        if ($Mode -eq 'migration-hard-exit-current' -and $script:migrationReplaceCount -eq 2) {
            [Environment]::Exit(95)
        }
    }
}
if ($Mode -like 'migration-hard-exit-*') {
    $env:USERPROFILE = Join-Path (Split-Path -Parent $ReadyPath) 'profile'
    Invoke-ArcGISProAgentManifestInstallWithDefaultAddInMigration -SourceRoot $SourceRoot `
        -InstallRoot $InstallRoot -AddInRoot $AddInRoot -Version '0.1.0' -Files $records `
        -OperationHook $hook | Out-Null
} else {
    Invoke-ArcGISProAgentManifestInstall -SourceRoot $SourceRoot -InstallRoot $InstallRoot -AddInRoot $AddInRoot `
        -Version '0.1.0' -Files $records -FailurePoint $FailurePoint -OperationHook $hook | Out-Null
}
'@
    [IO.File]::WriteAllText($path, $content, (New-Object Text.UTF8Encoding($false)))
    $path
}

function Start-InstallerChild {
    param($Case, [string]$Mode, [string]$ReadyPath, [string]$ReleasePath, [string]$FailurePoint = 'None')
    $childScript = New-InstallerChildScript $Case
    $arguments = @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', ('"' + $childScript + '"'),
        '-ModulePath', ('"' + $modulePath + '"'),
        '-SourceRoot', ('"' + $repoRoot + '"'),
        '-InstallRoot', ('"' + $Case.InstallRoot + '"'),
        '-AddInRoot', ('"' + $Case.AddInRoot + '"'),
        '-ReadyPath', ('"' + $ReadyPath + '"'),
        '-ReleasePath', ('"' + $ReleasePath + '"'),
        '-Mode', $Mode, '-FailurePoint', $FailurePoint
    )
    $startInfo = New-Object Diagnostics.ProcessStartInfo
    $startInfo.FileName = (Get-Command 'powershell.exe').Source
    $startInfo.Arguments = $arguments -join ' '
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $startInfo
    if (-not $process.Start()) { throw "Unable to start installer child process for mode $Mode" }
    $script:childProcesses.Add($process)
    $process
}

function Wait-TestReady {
    param([string]$Path, $Process, [string]$Message)
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while (-not (Test-Path -LiteralPath $Path) -and [DateTime]::UtcNow -lt $deadline) {
        $Process.Refresh()
        if ($Process.HasExited) {
            $details = $Process.StandardError.ReadToEnd()
            throw "$Message Child exited $($Process.ExitCode): $details"
        }
        Start-Sleep -Milliseconds 50
    }
    if (-not (Test-Path -LiteralPath $Path)) { throw "$Message Timed out." }
}

try {
    Invoke-Test 'Install-Dev uses the ArcGIS Pro GUID discovery root and direct package destination' {
        $expectedId = '{1A0481EA-3F43-4C98-B4B5-A58C727CD115}'
        $expectedRoot = Join-Path $env:USERPROFILE "Documents\ArcGIS\AddIns\ArcGISPro\$expectedId"
        $installerSource = Get-Text (Join-Path $repoRoot 'scripts\Install-Dev.ps1')
        $coreSource = Get-Text (Join-Path $repoRoot 'scripts\Install-Dev.Core.psm1')
        [xml]$config = Get-Text (Join-Path $repoRoot 'src\ArcGISProAgent.AddIn\Config.daml')
        $configId = [string]$config.ArcGIS.AddInInfo.id

        Assert-Equal $configId $expectedId 'Config.daml Add-In ID'
        Assert-True ($coreSource -match [regex]::Escape("`$script:AddInId = '$expectedId'")) `
            'the Add-In ID is a single core constant'
        Assert-True ($installerSource -match 'Get-ArcGISProAgentDefaultAddInRoot') `
            'Install-Dev obtains its default Add-In root from the core constant'
        Assert-True ($installerSource -match [regex]::Escape('(Join-Path $AddInRoot ''ArcGISProAgent.AddIn.esriAddinX'')')) `
            'Install-Dev places the package directly below AddInRoot'
        Assert-True ($installerSource -notmatch [regex]::Escape('$AddInRoot "$version\ArcGISProAgent.AddIn.esriAddinX"')) `
            'Install-Dev does not add a version directory below AddInRoot'
        Assert-Equal (Get-ArcGISProAgentDefaultAddInRoot) ([IO.Path]::GetFullPath($expectedRoot).TrimEnd('\')) `
            'default Add-In root is the official ArcGIS Pro GUID directory'
    }

    Invoke-Test 'Add-In package is direct below the owned root and an unmanaged package is refused' {
        $case = New-TestCase 'direct-addin-unmanaged-refusal'
        Set-TestArtifacts $case 'v1'
        $addinRecord = @(Get-TestRecords $case | Where-Object { $_.Component -eq 'addin' })[0]
        Assert-Equal $addinRecord.DestinationPath (Join-Path $case.AddInRoot 'ArcGISProAgent.AddIn.esriAddinX') `
            'direct Add-In destination'
        New-Item -ItemType Directory -Force -Path $case.AddInRoot | Out-Null
        [IO.File]::WriteAllText($addinRecord.DestinationPath, 'unmanaged-same-guid-package')
        Assert-Throws { Invoke-TestInstall $case } 'not owned|unowned' 'unmanaged package at official destination is refused'
        Assert-Equal (Get-Text $addinRecord.DestinationPath) 'unmanaged-same-guid-package' 'unmanaged Add-In is preserved'
        Assert-True (-not (Test-Path -LiteralPath $case.Manifest)) 'refusal does not create a manifest'
    }

    Invoke-Test 'exact legacy default migration removes only its owned Add-In and reinstalls all ownership' {
        $case = New-TestCase 'legacy-default-migration'
        Assert-True ($null -ne (Get-Command Invoke-ArcGISProAgentManifestInstallWithDefaultAddInMigration -ErrorAction SilentlyContinue)) `
            'the exact default-root migration entry point exists'
        $oldProfile = $env:USERPROFILE
        try {
            $env:USERPROFILE = Join-Path $case.Base 'profile'
            $legacyRoot = Get-ArcGISProAgentLegacyDefaultAddInRoot
            $case.AddInRoot = Get-ArcGISProAgentDefaultAddInRoot
            Set-TestArtifacts $case 'v2'
            $legacy = New-LegacyInstallFixture -Case $case -LegacyAddInRoot $legacyRoot -Content 'v1'

            Invoke-ArcGISProAgentManifestInstallWithDefaultAddInMigration -SourceRoot $repoRoot `
                -InstallRoot $case.InstallRoot -AddInRoot $case.AddInRoot -Version '0.1.0' `
                -Files (Get-TestRecords $case) | Out-Null

            Assert-True (-not (Test-Path -LiteralPath $legacy.AddIn)) 'manifest-owned legacy Add-In is removed'
            Assert-Equal (Get-Text $legacy.Mcp) 'mcp-v2' 'MCP is reinstalled and remains owned'
            Assert-Equal (Get-Text $legacy.Desktop) 'desktop-v2' 'desktop is reinstalled and remains owned'
            Assert-Equal (Get-Text (Join-Path $case.AddInRoot 'ArcGISProAgent.AddIn.esriAddinX')) 'addin-v2' `
                'Add-In is installed directly in the official root'
            $manifest = Get-Content -LiteralPath $case.Manifest -Raw | ConvertFrom-Json
            Assert-Equal $manifest.addInRoot ([IO.Path]::GetFullPath($case.AddInRoot).TrimEnd('\')) 'manifest switches to official root'
            Assert-Equal @($manifest.files).Count 3 'final manifest owns MCP, desktop, and Add-In'
        } finally {
            $env:USERPROFILE = $oldProfile
        }
    }

    Invoke-Test 'root mismatch with a remaining legacy Add-In entry fails closed' {
        $case = New-TestCase 'legacy-root-mismatch-with-addin'
        $oldProfile = $env:USERPROFILE
        try {
            $env:USERPROFILE = Join-Path $case.Base 'profile'
            $legacyRoot = Get-ArcGISProAgentLegacyDefaultAddInRoot
            $case.AddInRoot = Get-ArcGISProAgentDefaultAddInRoot
            Set-TestArtifacts $case 'v2'
            $legacy = New-LegacyInstallFixture -Case $case -LegacyAddInRoot $legacyRoot -Content 'v1'
            $manifestBefore = Get-Text $case.Manifest
            Assert-Throws {
                Invoke-ArcGISProAgentManifestInstall -SourceRoot $repoRoot -InstallRoot $case.InstallRoot `
                    -AddInRoot $case.AddInRoot -PreviousAddInRootWithoutEntries $legacyRoot -Version '0.1.0' `
                    -Files (Get-TestRecords $case) | Out-Null
            } 'only after every legacy Add-In entry is removed' 'root switch with remaining Add-In ownership must fail'
            Assert-Equal (Get-Text $legacy.AddIn) 'addin-v1' 'legacy Add-In remains untouched on refusal'
            Assert-Equal (Get-Text $case.Manifest) $manifestBefore 'legacy manifest remains untouched on refusal'
        } finally {
            $env:USERPROFILE = $oldProfile
        }
    }

    Invoke-Test 'failure in either legacy migration transaction is recoverable and rerun converges' {
        foreach ($failure in @(
            [PSCustomObject]@{ Name='legacy'; Legacy='Stale'; Current='None' },
            [PSCustomObject]@{ Name='current'; Legacy='None'; Current='Copy' }
        )) {
            $case = New-TestCase ("migration-failure-" + $failure.Name)
            $oldProfile = $env:USERPROFILE
            try {
                $env:USERPROFILE = Join-Path $case.Base 'profile'
                $legacyRoot = Get-ArcGISProAgentLegacyDefaultAddInRoot
                $case.AddInRoot = Get-ArcGISProAgentDefaultAddInRoot
                Set-TestArtifacts $case 'v2'
                $legacy = New-LegacyInstallFixture -Case $case -LegacyAddInRoot $legacyRoot -Content 'v1'
                Assert-Throws {
                    Invoke-ArcGISProAgentManifestInstallWithDefaultAddInMigration -SourceRoot $repoRoot `
                        -InstallRoot $case.InstallRoot -AddInRoot $case.AddInRoot -Version '0.1.0' `
                        -Files (Get-TestRecords $case) -LegacyFailurePoint $failure.Legacy `
                        -FailurePoint $failure.Current | Out-Null
                } 'injected' "$($failure.Name) migration transaction failure is surfaced"
                Assert-NoTransactionArtifacts $case

                Invoke-ArcGISProAgentManifestInstallWithDefaultAddInMigration -SourceRoot $repoRoot `
                    -InstallRoot $case.InstallRoot -AddInRoot $case.AddInRoot -Version '0.1.0' `
                    -Files (Get-TestRecords $case) | Out-Null
                Assert-True (-not (Test-Path -LiteralPath $legacy.AddIn)) "$($failure.Name) rerun removes legacy Add-In"
                Assert-Equal (Get-Text (Join-Path $case.AddInRoot 'ArcGISProAgent.AddIn.esriAddinX')) 'addin-v2' `
                    "$($failure.Name) rerun installs official Add-In"
                Assert-Equal @((Get-Content -LiteralPath $case.Manifest -Raw | ConvertFrom-Json).files).Count 3 `
                    "$($failure.Name) rerun converges to full ownership"
                Assert-NoTransactionArtifacts $case
            } finally {
                $env:USERPROFILE = $oldProfile
            }
        }
    }

    Invoke-Test 'migration wrapper recovers a hard exit from the legacy transaction in one rerun' {
        $case = New-TestCase 'migration-hard-exit-legacy'
        $oldProfile = $env:USERPROFILE
        try {
            $env:USERPROFILE = Join-Path $case.Base 'profile'
            $legacyRoot = Get-ArcGISProAgentLegacyDefaultAddInRoot
            $case.AddInRoot = Get-ArcGISProAgentDefaultAddInRoot
            Set-TestArtifacts $case 'v2'
            $legacy = New-LegacyInstallFixture -Case $case -LegacyAddInRoot $legacyRoot -Content 'v1'
            $child = Start-InstallerChild -Case $case -Mode 'migration-hard-exit-legacy' `
                -ReadyPath (Join-Path $case.Base 'unused-ready') -ReleasePath (Join-Path $case.Base 'unused-release')
            Assert-True ($child.WaitForExit(30000)) 'legacy migration hard-exit child exits promptly'
            Assert-Equal $child.ExitCode 94 'legacy migration child hard exits after its persisted Replace journal'
            $journal = Register-CaseStageFromJournal -Case $case -JournalAddInRoot $legacyRoot

            Invoke-ArcGISProAgentManifestInstallWithDefaultAddInMigration -SourceRoot $repoRoot `
                -InstallRoot $case.InstallRoot -AddInRoot $case.AddInRoot -Version '0.1.0' `
                -Files (Get-TestRecords $case) | Out-Null

            Assert-MigrationConverged -Case $case -Legacy $legacy -StageRoot ([string]$journal.stageRoot) `
                -Context 'legacy hard-exit recovery'
        } finally {
            $env:USERPROFILE = $oldProfile
        }
    }

    Invoke-Test 'migration wrapper recovers a hard exit from the current transaction in one rerun' {
        $case = New-TestCase 'migration-hard-exit-current'
        $oldProfile = $env:USERPROFILE
        try {
            $env:USERPROFILE = Join-Path $case.Base 'profile'
            $legacyRoot = Get-ArcGISProAgentLegacyDefaultAddInRoot
            $case.AddInRoot = Get-ArcGISProAgentDefaultAddInRoot
            Set-TestArtifacts $case 'v2'
            $legacy = New-LegacyInstallFixture -Case $case -LegacyAddInRoot $legacyRoot -Content 'v1'
            $child = Start-InstallerChild -Case $case -Mode 'migration-hard-exit-current' `
                -ReadyPath (Join-Path $case.Base 'unused-ready') -ReleasePath (Join-Path $case.Base 'unused-release')
            Assert-True ($child.WaitForExit(30000)) 'current migration hard-exit child exits promptly'
            Assert-Equal $child.ExitCode 95 'current migration child hard exits after its persisted Replace journal'
            $journal = Register-CaseStageFromJournal -Case $case -JournalAddInRoot $case.AddInRoot

            Invoke-ArcGISProAgentManifestInstallWithDefaultAddInMigration -SourceRoot $repoRoot `
                -InstallRoot $case.InstallRoot -AddInRoot $case.AddInRoot -Version '0.1.0' `
                -Files (Get-TestRecords $case) | Out-Null

            Assert-MigrationConverged -Case $case -Legacy $legacy -StageRoot ([string]$journal.stageRoot) `
                -Context 'current hard-exit recovery'
        } finally {
            $env:USERPROFILE = $oldProfile
        }
    }

    Invoke-Test 'double run, strict manifest, hashes, and atomic manifest replacement' {
        $case = New-TestCase 'double-run'
        Set-TestArtifacts $case 'v1'
        Invoke-TestInstall $case
        Invoke-TestInstall $case
        $manifest = Get-Content -LiteralPath $case.Manifest -Raw | ConvertFrom-Json
        Assert-Equal $manifest.schemaVersion 1 'schemaVersion'
        Assert-Equal $manifest.owner 'ArcGISProAgent' 'manifest owner'
        Assert-Equal $manifest.version '0.1.0' 'manifest version'
        Assert-Equal $manifest.manifestPath ([IO.Path]::GetFullPath($case.Manifest)) 'canonical manifest path'
        Assert-Equal $manifest.installRoot ([IO.Path]::GetFullPath($case.InstallRoot).TrimEnd('\')) 'canonical install root'
        Assert-Equal $manifest.addInRoot ([IO.Path]::GetFullPath($case.AddInRoot).TrimEnd('\')) 'canonical Add-In root'
        Assert-Equal @($manifest.files).Count 3 'manifest entry count'
        foreach ($entry in @($manifest.files)) {
            Assert-Equal $entry.owner 'ArcGISProAgent' 'entry owner'
            Assert-Equal $entry.version '0.1.0' 'entry version'
            Assert-True ([IO.Path]::IsPathRooted([string]$entry.path)) 'entry path is fully qualified'
            Assert-True ([string]$entry.sha256 -match '^[0-9a-f]{64}$') 'entry SHA-256 shape'
            Assert-Equal ([long](Get-Item -LiteralPath $entry.path).Length) ([long]$entry.length) 'entry length'
            Assert-Equal (Get-FileHash -LiteralPath $entry.path -Algorithm SHA256).Hash.ToLowerInvariant() $entry.sha256 'entry hash'
        }

        $oldManifest = Get-Text $case.Manifest
        $hardLink = Join-Path $case.Base 'old-manifest-hardlink.json'
        New-Item -ItemType HardLink -Path $hardLink -Target $case.Manifest | Out-Null
        Set-TestArtifacts $case 'v2'
        Invoke-TestInstall $case
        Assert-Equal (Get-Text $hardLink) $oldManifest 'manifest hardlink must retain old bytes'
        Assert-True ((Get-Text $case.Manifest) -ne $oldManifest) 'manifest must be atomically replaced'
        Assert-NoTransactionArtifacts $case
    }

    Invoke-Test 'first install creates only the missing tails of explicit default-like roots' {
        $case = New-TestCase 'default-like-missing-root-tails'
        $case.InstallRoot = Join-Path $case.Base 'Local\ArcGISProAgent\dev'
        $case.AddInRoot = Join-Path $case.Base 'Documents\ArcGIS\AddIns\ArcGISProAgent'
        $case.Manifest = Join-Path $case.InstallRoot 'install-manifest.json'
        Set-TestArtifacts $case 'v1'
        Assert-True (-not (Test-Path -LiteralPath (Join-Path $case.Base 'Local'))) 'default-like Local tail starts absent'
        Assert-True (-not (Test-Path -LiteralPath (Join-Path $case.Base 'Documents'))) 'default-like Documents tail starts absent'

        Invoke-TestInstall $case
        Invoke-TestInstall $case

        Assert-Equal (Get-Text ((Get-TestRecords $case)[0].DestinationPath)) 'mcp-v1' 'default-like MCP target after double install'
        Assert-Equal (Get-Text ((Get-TestRecords $case)[1].DestinationPath)) 'desktop-v1' 'default-like desktop target after double install'
        Assert-True (Test-Path -LiteralPath $case.Manifest -PathType Leaf) 'default-like manifest exists'
        Assert-NoTransactionArtifacts $case
    }

    Invoke-Test 'authorized root creation rejects an intermediate tail replaced by a junction' {
        $case = New-TestCase 'authorized-root-junction-race'
        $case.InstallRoot = Join-Path $case.Base 'Local\ArcGISProAgent\dev'
        $case.AddInRoot = Join-Path $case.Base 'Documents\ArcGIS\AddIns\ArcGISProAgent'
        $case.Manifest = Join-Path $case.InstallRoot 'install-manifest.json'
        Set-TestArtifacts $case 'v1'
        $raceSegment = Join-Path $case.Base 'Local'
        $external = Join-Path $case.Base 'external-junction-target'
        New-Item -ItemType Directory -Path $external | Out-Null
        $sentinel = Join-Path $external 'sentinel.txt'
        [IO.File]::WriteAllText($sentinel, 'keep')
        $hookState = [PSCustomObject]@{ Triggered=$false }
        $hook = {
            param([string]$EventName, $Context)
            if ($EventName -eq 'AfterAuthorizedRootSegmentCreate' -and
                [string]$Context.Path -eq [IO.Path]::GetFullPath($raceSegment).TrimEnd('\')) {
                $hookState.Triggered = $true
                [IO.Directory]::Delete($raceSegment, $false)
                New-Item -ItemType Junction -Path $raceSegment -Target $external | Out-Null
                $script:junctions.Add([IO.Path]::GetFullPath($raceSegment))
            }
        }
        Assert-Throws {
            Invoke-ArcGISProAgentManifestInstall -SourceRoot $repoRoot -InstallRoot $case.InstallRoot `
                -AddInRoot $case.AddInRoot -Version '0.1.0' -Files (Get-TestRecords $case) `
                -OperationHook $hook | Out-Null
        } 'reparse|junction|entity|changed|race' 'junction replacement of an authorized missing-tail segment must fail closed'
        Assert-True $hookState.Triggered 'authorized-root race hook was reached before rejection'
        Assert-Equal (Get-Text $sentinel) 'keep' 'authorized-root junction sentinel remains unchanged'
        Assert-True (-not (Test-Path -LiteralPath (Join-Path $external 'ArcGISProAgent'))) `
            'authorized-root race does not create the next segment through the junction'
    }

    Invoke-Test 'unowned target is preserved and rejected' {
        $case = New-TestCase 'unowned'
        Set-TestArtifacts $case 'v1'
        $target = (Get-TestRecords $case)[0].DestinationPath
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
        [IO.File]::WriteAllText($target, 'user-owned')
        Assert-Throws { Invoke-TestInstall $case } 'not owned|unowned' 'unowned target rejection'
        Assert-Equal (Get-Text $target) 'user-owned' 'unowned target preserved'
        Assert-True (-not (Test-Path -LiteralPath $case.Manifest)) 'manifest not created on rejection'
    }

    Invoke-Test 'tampered owned target and rebuilt manifest are preserved and rejected' {
        $case = New-TestCase 'tampered'
        Set-TestArtifacts $case 'v1'
        Invoke-TestInstall $case
        $manifestBefore = Get-Text $case.Manifest
        $target = (Get-TestRecords $case)[0].DestinationPath
        [IO.File]::WriteAllText($target, 'tampered-user-bytes')
        Assert-Throws { Invoke-TestInstall $case } 'hash|length|changed|tamper' 'tampered owned target rejection'
        Assert-Equal (Get-Text $target) 'tampered-user-bytes' 'tampered target preserved'
        Assert-Equal (Get-Text $case.Manifest) $manifestBefore 'manifest preserved after target rejection'

        [IO.File]::WriteAllText($target, 'mcp-v1')
        $rebuilt = Get-Content -LiteralPath $case.Manifest -Raw | ConvertFrom-Json
        $rebuilt.files[0].sha256 = ('0' * 64)
        $rebuiltText = $rebuilt | ConvertTo-Json -Depth 6
        [IO.File]::WriteAllText($case.Manifest, $rebuiltText)
        Assert-Throws { Invoke-TestInstall $case } 'hash|manifest' 'rebuilt manifest rejection'
        Assert-Equal (Get-Text $case.Manifest) $rebuiltText 'rebuilt manifest preserved'
    }

    Invoke-Test 'stale files are transactional and tampered stale files fail closed' {
        $case = New-TestCase 'stale-success'
        Set-TestArtifacts $case 'v1'
        Invoke-TestInstall $case
        $stale = (Get-TestRecords $case)[1].DestinationPath
        Invoke-TestInstall -Case $case -Records (Get-TestRecords $case -WithoutDesktop)
        Assert-True (-not (Test-Path -LiteralPath $stale)) 'verified stale file removed'
        Assert-Equal @((Get-Content -LiteralPath $case.Manifest -Raw | ConvertFrom-Json).files).Count 2 'stale manifest entry removed'

        $tampered = New-TestCase 'stale-tampered'
        Set-TestArtifacts $tampered 'v1'
        Invoke-TestInstall $tampered
        $tamperedStale = (Get-TestRecords $tampered)[1].DestinationPath
        [IO.File]::WriteAllText($tamperedStale, 'user-modified-stale')
        $manifestBefore = Get-Text $tampered.Manifest
        Assert-Throws {
            Invoke-TestInstall -Case $tampered -Records (Get-TestRecords $tampered -WithoutDesktop)
        } 'hash|length|changed|tamper' 'tampered stale rejection'
        Assert-Equal (Get-Text $tamperedStale) 'user-modified-stale' 'tampered stale preserved'
        Assert-Equal (Get-Text $tampered.Manifest) $manifestBefore 'manifest preserved for tampered stale'
    }

    Invoke-Test 'GIS container ancestors and sidecars are rejected' {
        foreach ($containerName in @('project.gdb', 'connection.sde', 'data.geodatabase', 'legacy.mdb', 'roads.shp')) {
            $case = New-TestCase ("gis-" + $containerName.Replace('.', '-'))
            Set-TestArtifacts $case 'v1'
            $container = Join-Path $case.Base $containerName
            New-Item -ItemType Directory -Force -Path $container | Out-Null
            $sentinel = Join-Path $container 'sentinel.txt'
            [IO.File]::WriteAllText($sentinel, 'keep')
            $case.InstallRoot = Join-Path $container 'install'
            $case.Manifest = Join-Path $case.InstallRoot 'install-manifest.json'
            Assert-Throws { Invoke-TestInstall $case } 'GIS|data|container|extension' "reject $containerName ancestor"
            Assert-Equal (Get-Text $sentinel) 'keep' "$containerName sentinel preserved"
        }
    }

    Invoke-Test 'root overlap and source containment are rejected' {
        $case = New-TestCase 'root-overlap'
        Set-TestArtifacts $case 'v1'
        $case.AddInRoot = Join-Path $case.InstallRoot 'nested-addin'
        Assert-Throws { Invoke-TestInstall $case } 'overlap' 'overlapping roots rejected'

        $inside = New-TestCase 'inside-source'
        Set-TestArtifacts $inside 'v1'
        Assert-Throws { Invoke-TestInstall -Case $inside -SourceRoot $inside.Base } 'source' 'destination inside source rejected'

        $contains = New-TestCase 'contains-source'
        Set-TestArtifacts $contains 'v1'
        $containsSourceRoot = Join-Path $contains.InstallRoot 'repo-source'
        Assert-Throws { Invoke-TestInstall -Case $contains -SourceRoot $containsSourceRoot } 'source' 'destination containing source rejected'
    }

    Invoke-Test 'Windows entity paths reject SUBST source containment and root overlap' {
        $sourceAlias = New-TestCase 'subst-source-containment'
        Set-TestArtifacts $sourceAlias 'v1'
        $drive = Set-TestSubst -Target $sourceAlias.Source
        $sourceAlias.InstallRoot = Join-Path ($drive + '\') 'nested-install'
        $sourceAlias.Manifest = Join-Path $sourceAlias.InstallRoot 'install-manifest.json'
        Assert-Throws {
            Invoke-TestInstall -Case $sourceAlias -SourceRoot $sourceAlias.Source
        } 'source|entity|contain' 'SUBST path into source must be rejected'

        & subst.exe $script:substDrive /d | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "Unable to remove test SUBST drive: $script:substDrive" }
        $script:substDrive = $null

        $overlapAlias = New-TestCase 'subst-root-overlap'
        Set-TestArtifacts $overlapAlias 'v1'
        New-Item -ItemType Directory -Force -Path $overlapAlias.InstallRoot | Out-Null
        $drive = Set-TestSubst -Target $overlapAlias.InstallRoot
        $overlapAlias.AddInRoot = Join-Path ($drive + '\') 'nested-addin'
        Assert-Throws { Invoke-TestInstall $overlapAlias } 'overlap|entity' 'SUBST root overlap must be rejected'
    }

    Invoke-Test 'Windows entity paths treat an available 8.3 alias as the same source' {
        $shortCase = New-TestCase 'eight-dot-three'
        $longSource = Join-Path $shortCase.Base 'Long Source Directory For Entity Alias Test'
        New-Item -ItemType Directory -Force -Path $longSource | Out-Null
        $shortCase.Source = $longSource
        Set-TestArtifacts $shortCase 'v1'
        $shortSource = Get-ShortPathIfAvailable $longSource
        if ([string]::IsNullOrWhiteSpace($shortSource) -or
            $shortSource.Equals($longSource, [StringComparison]::OrdinalIgnoreCase) -or
            $shortSource -notmatch '~') {
            Write-Host 'SKIP 8.3 entity-alias assertion: the TEMP volume does not expose a distinct short name.' -ForegroundColor Yellow
        } else {
            $shortCase.InstallRoot = Join-Path $shortSource 'nested-install'
            $shortCase.Manifest = Join-Path $shortCase.InstallRoot 'install-manifest.json'
            Assert-Throws {
                Invoke-TestInstall -Case $shortCase -SourceRoot $longSource
            } 'source|entity|contain' '8.3 path into source must be rejected'
        }
    }

    Invoke-Test 'exclusive installer lock rejects a concurrent installer' {
        $lockCase = New-TestCase 'exclusive-lock'
        Set-TestArtifacts $lockCase 'v1'
        $ready = Join-Path $lockCase.Base 'lock-ready'
        $release = Join-Path $lockCase.Base 'lock-release'
        $child = Start-InstallerChild -Case $lockCase -Mode 'hold-lock' -ReadyPath $ready -ReleasePath $release
        Wait-TestReady -Path $ready -Process $child -Message 'Child did not acquire the installer lock.'
        Assert-Throws { Invoke-TestInstall $lockCase } 'lock|installer.+running|concurrent' 'concurrent installer must be rejected by the exclusive lock'
        [IO.File]::WriteAllText($release, 'release')
        Assert-True ($child.WaitForExit(30000)) 'lock-holding child exits after release'
        Assert-Equal $child.ExitCode 0 'lock-holding child exit code'
    }

    Invoke-Test 'next installer rolls back a process exit after the first replace and succeeds' {
        $crashCase = New-TestCase 'crash-recovery'
        Set-TestArtifacts $crashCase 'v1'
        Invoke-TestInstall $crashCase
        $oldTarget = (Get-TestRecords $crashCase)[0].DestinationPath
        Assert-Equal (Get-Text $oldTarget) 'mcp-v1' 'crash baseline target'
        Set-TestArtifacts $crashCase 'v2'

        $unrelatedTransactionId = [Guid]::NewGuid().ToString('N')
        $unrelatedStage = Join-Path $tempRoot "ArcGISProAgent-install-$unrelatedTransactionId"
        New-Item -ItemType Directory -Path $unrelatedStage | Out-Null
        $script:otherTempPaths.Add($unrelatedStage)
        $unrelatedSentinel = Join-Path $unrelatedStage 'unrelated-sentinel.txt'
        [IO.File]::WriteAllText($unrelatedSentinel, 'keep')
        $ready = Join-Path $crashCase.Base 'crash-ready-unused'
        $release = Join-Path $crashCase.Base 'crash-release-unused'
        $child = Start-InstallerChild -Case $crashCase -Mode 'crash-after-replace' -ReadyPath $ready -ReleasePath $release
        Assert-True ($child.WaitForExit(30000)) 'crash child exits promptly'
        Assert-Equal $child.ExitCode 91 'crash child exits at the first replace hook'
        Register-CaseStageFromJournal $crashCase | Out-Null
        $journals = @(Get-ChildItem -LiteralPath $crashCase.InstallRoot -Filter '.arcgis-pro-agent-install-journal*.json' -File -Force)
        Assert-Equal $journals.Count 1 'crash leaves one durable transaction journal'
        Assert-True (@(Get-ChildItem -LiteralPath $crashCase.InstallRoot -Recurse -Filter '.arcgis-pro-agent-backup-*' -File -Force).Count -ge 1) 'crash leaves the first replacement backup'

        Invoke-TestInstall $crashCase
        Assert-Equal (Get-Text $oldTarget) 'mcp-v2' 'next installer recovers then installs v2'
        Assert-Equal (Get-Text $unrelatedSentinel) 'keep' 'recovery leaves an unrelated concurrent TEMP stage untouched'
        Assert-NoTransactionArtifacts $crashCase
    }

    Invoke-Test 'recovery rejects a tampered Replace backup before persisting inferred state' {
        $case = New-TestCase 'tampered-crash-replace-backup'
        Set-TestArtifacts $case 'v1'
        Invoke-TestInstall $case
        Set-TestArtifacts $case 'v2'
        $child = Start-InstallerChild -Case $case -Mode 'crash-before-backup-state-replace' `
            -ReadyPath (Join-Path $case.Base 'unused-ready') -ReleasePath (Join-Path $case.Base 'unused-release')
        Assert-True ($child.WaitForExit(30000)) 'Replace crash child exits promptly'
        Assert-Equal $child.ExitCode 91 'Replace crash occurs before backup state persistence'
        $journal = Register-CaseStageFromJournal $case
        $operation = @($journal.operations | Where-Object { $_.kind -eq 'Replace' -and [string]::IsNullOrWhiteSpace([string]$_.backupIdentity) })[0]
        Assert-True ($null -ne $operation) 'journal contains the interrupted Replace operation'
        [IO.File]::WriteAllText([string]$operation.backup, 'tampered-replace-backup')
        $targetBefore = Get-Text ([string]$operation.target)
        $backupBefore = Get-Text ([string]$operation.backup)
        $journalPath = Join-Path $case.InstallRoot '.arcgis-pro-agent-install-journal.json'
        $journalBefore = Get-Text $journalPath
        Assert-Throws { Invoke-TestInstall $case } 'manual review|persisted old state|backup' `
            'recovery must reject a Replace backup that does not match persisted old state'
        Assert-Equal (Get-Text ([string]$operation.target)) $targetBefore 'Replace target bytes remain unchanged'
        Assert-Equal (Get-Text ([string]$operation.backup)) $backupBefore 'tampered Replace backup remains unchanged'
        Assert-Equal (Get-Text $journalPath) $journalBefore 'Replace journal bytes remain unchanged'
    }

    Invoke-Test 'recovery rejects a tampered ManifestReplace backup before persisting inferred state' {
        $case = New-TestCase 'tampered-crash-manifest-backup'
        Set-TestArtifacts $case 'v1'
        Invoke-TestInstall $case
        Set-TestArtifacts $case 'v2'
        $child = Start-InstallerChild -Case $case -Mode 'crash-before-backup-state-manifest' `
            -ReadyPath (Join-Path $case.Base 'unused-ready') -ReleasePath (Join-Path $case.Base 'unused-release')
        Assert-True ($child.WaitForExit(30000)) 'ManifestReplace crash child exits promptly'
        Assert-Equal $child.ExitCode 92 'ManifestReplace crash occurs before backup state persistence'
        $journal = Register-CaseStageFromJournal $case
        $operation = @($journal.operations | Where-Object { $_.kind -eq 'ManifestReplace' -and [string]::IsNullOrWhiteSpace([string]$_.backupIdentity) })[0]
        Assert-True ($null -ne $operation) 'journal contains the interrupted ManifestReplace operation'
        [IO.File]::WriteAllText([string]$operation.backup, 'tampered-manifest-backup')
        $targetBefore = Get-Text ([string]$operation.target)
        $backupBefore = Get-Text ([string]$operation.backup)
        $journalPath = Join-Path $case.InstallRoot '.arcgis-pro-agent-install-journal.json'
        $journalBefore = Get-Text $journalPath
        Assert-Throws { Invoke-TestInstall $case } 'manual review|persisted old state|backup' `
            'recovery must reject a ManifestReplace backup that does not match persisted old state'
        Assert-Equal (Get-Text ([string]$operation.target)) $targetBefore 'ManifestReplace target bytes remain unchanged'
        Assert-Equal (Get-Text ([string]$operation.backup)) $backupBefore 'tampered ManifestReplace backup remains unchanged'
        Assert-Equal (Get-Text $journalPath) $journalBefore 'ManifestReplace journal bytes remain unchanged'
    }

    Invoke-Test 'recovery rejects a tampered Stale backup before persisting inferred state' {
        $case = New-TestCase 'tampered-crash-stale-backup'
        Set-TestArtifacts $case 'v1'
        Invoke-TestInstall $case
        $child = Start-InstallerChild -Case $case -Mode 'crash-before-backup-state-stale' `
            -ReadyPath (Join-Path $case.Base 'unused-ready') -ReleasePath (Join-Path $case.Base 'unused-release')
        Assert-True ($child.WaitForExit(30000)) 'Stale crash child exits promptly'
        Assert-Equal $child.ExitCode 93 'Stale crash occurs before backup state persistence'
        $journal = Register-CaseStageFromJournal $case
        $operation = @($journal.operations | Where-Object { $_.kind -eq 'Stale' -and [string]::IsNullOrWhiteSpace([string]$_.backupIdentity) })[0]
        Assert-True ($null -ne $operation) 'journal contains the interrupted Stale operation'
        Assert-True (-not (Test-Path -LiteralPath ([string]$operation.target))) 'interrupted stale target remains moved'
        [IO.File]::WriteAllText([string]$operation.backup, 'tampered-stale-backup')
        $backupBefore = Get-Text ([string]$operation.backup)
        $journalPath = Join-Path $case.InstallRoot '.arcgis-pro-agent-install-journal.json'
        $journalBefore = Get-Text $journalPath
        Assert-Throws { Invoke-TestInstall -Case $case -Records (Get-TestRecords $case -WithoutDesktop) } `
            'manual review|persisted old state|backup' `
            'recovery must reject a Stale backup that does not match persisted old state'
        Assert-True (-not (Test-Path -LiteralPath ([string]$operation.target))) 'Stale target remains absent'
        Assert-Equal (Get-Text ([string]$operation.backup)) $backupBefore 'tampered Stale backup remains unchanged'
        Assert-Equal (Get-Text $journalPath) $journalBefore 'Stale journal bytes remain unchanged'
    }

    Invoke-Test 'a raced unowned New target is preserved and never rollback-deleted' {
        $raceCase = New-TestCase 'race-new'
        Set-TestArtifacts $raceCase 'v1'
        $target = (Get-TestRecords $raceCase)[0].DestinationPath
        $hook = {
            param([string]$EventName, $Context)
            if ($EventName -eq 'BeforeNewMove' -and $Context.Target -eq $target) {
                [IO.File]::WriteAllText($Context.Target, 'raced-user-file')
            }
        }
        Assert-Throws {
            Invoke-ArcGISProAgentManifestInstall -SourceRoot $repoRoot -InstallRoot $raceCase.InstallRoot `
                -AddInRoot $raceCase.AddInRoot -Version '0.1.0' -Files (Get-TestRecords $raceCase) `
                -OperationHook $hook | Out-Null
        } 'exist|unowned|race|move' 'raced New move must fail'
        Assert-Equal (Get-Text $target) 'raced-user-file' 'raced unowned target is preserved'
    }

    Invoke-Test 'replace backup verification restores concurrently modified user bytes' {
        $raceCase = New-TestCase 'race-replace'
        Set-TestArtifacts $raceCase 'v1'
        Invoke-TestInstall $raceCase
        $target = (Get-TestRecords $raceCase)[0].DestinationPath
        $manifestBefore = Get-Text $raceCase.Manifest
        Set-TestArtifacts $raceCase 'v2'
        $hook = {
            param([string]$EventName, $Context)
            if ($EventName -eq 'BeforeReplace' -and $Context.Target -eq $target) {
                [IO.File]::WriteAllText($Context.Target, 'concurrent-user-bytes')
            }
        }
        Assert-Throws {
            Invoke-ArcGISProAgentManifestInstall -SourceRoot $repoRoot -InstallRoot $raceCase.InstallRoot `
                -AddInRoot $raceCase.AddInRoot -Version '0.1.0' -Files (Get-TestRecords $raceCase) `
                -OperationHook $hook | Out-Null
        } 'backup|changed|concurrent|hash|length' 'concurrent Replace mutation must fail backup verification'
        Assert-Equal (Get-Text $target) 'concurrent-user-bytes' 'backup bytes are restored to the target'
        Assert-Equal (Get-Text $raceCase.Manifest) $manifestBefore 'old manifest remains after concurrent mutation'
    }

    Invoke-Test 'DLL and EXE hard links retain old bytes when product targets are replaced' {
        $linkCase = New-TestCase 'product-hardlinks'
        Set-TestArtifacts $linkCase 'v1'
        Invoke-TestInstall $linkCase
        $records = Get-TestRecords $linkCase
        $dllLink = Join-Path $linkCase.Base 'Agent-old-link.dll'
        $exeLink = Join-Path $linkCase.Base 'agent-old-link.exe'
        New-Item -ItemType HardLink -Path $dllLink -Target $records[0].DestinationPath | Out-Null
        New-Item -ItemType HardLink -Path $exeLink -Target $records[1].DestinationPath | Out-Null
        Set-TestArtifacts $linkCase 'v2'
        Invoke-TestInstall $linkCase
        Assert-Equal (Get-Text $dllLink) 'mcp-v1' 'DLL hard link retains old bytes'
        Assert-Equal (Get-Text $exeLink) 'desktop-v1' 'EXE hard link retains old bytes'
        Assert-Equal (Get-Text $records[0].DestinationPath) 'mcp-v2' 'DLL target has new bytes'
        Assert-Equal (Get-Text $records[1].DestinationPath) 'desktop-v2' 'EXE target has new bytes'
    }

    Invoke-Test 'cleanup failure retains committed journal and the next run finishes cleanup' {
        $cleanupCase = New-TestCase 'cleanup-recovery'
        Set-TestArtifacts $cleanupCase 'v1'
        Invoke-TestInstall $cleanupCase
        Set-TestArtifacts $cleanupCase 'v2'
        $holder = [PSCustomObject]@{ Stream = $null }
        $hook = {
            param([string]$EventName, $Context)
            if ($EventName -eq 'BeforeCommittedCleanup' -and $null -eq $holder.Stream) {
                $holder.Stream = New-Object IO.FileStream(
                    [string]$Context.Backup,
                    [IO.FileMode]::Open,
                    [IO.FileAccess]::Read,
                    [IO.FileShare]::Read)
            }
        }
        try {
            Assert-Throws {
                Invoke-ArcGISProAgentManifestInstall -SourceRoot $repoRoot -InstallRoot $cleanupCase.InstallRoot `
                    -AddInRoot $cleanupCase.AddInRoot -Version '0.1.0' -Files (Get-TestRecords $cleanupCase) `
                    -OperationHook $hook | Out-Null
            } 'cleanup|used by another process|access|denied|sharing' 'real backup cleanup failure'
        } finally {
            if ($null -ne $holder.Stream) { $holder.Stream.Dispose() }
        }
        $journal = Join-Path $cleanupCase.InstallRoot '.arcgis-pro-agent-install-journal.json'
        Assert-True (Test-Path -LiteralPath $journal -PathType Leaf) 'committed cleanup journal is retained'
        Assert-Equal (Get-Text ((Get-TestRecords $cleanupCase)[0].DestinationPath)) 'mcp-v2' 'committed file is not rolled back'
        Assert-Equal (Get-Text $cleanupCase.Manifest | ConvertFrom-Json).version '0.1.0' 'committed manifest remains'
        Invoke-TestInstall $cleanupCase
        Assert-Equal (Get-Text ((Get-TestRecords $cleanupCase)[0].DestinationPath)) 'mcp-v2' 'next run keeps committed bytes'
        Assert-NoTransactionArtifacts $cleanupCase
    }

    Invoke-Test 'invalid fixed recovery journal is preserved and fails closed' {
        $journalCase = New-TestCase 'invalid-journal'
        Set-TestArtifacts $journalCase 'v1'
        New-Item -ItemType Directory -Force -Path $journalCase.InstallRoot | Out-Null
        $journal = Join-Path $journalCase.InstallRoot '.arcgis-pro-agent-install-journal.json'
        [IO.File]::WriteAllText($journal, '{"schemaVersion":2,"owner":"WrongOwner"}')
        Assert-Throws { Invoke-TestInstall $journalCase } 'journal|owner|schema|required' 'invalid recovery journal fails closed'
        Assert-Equal (Get-Text $journal) '{"schemaVersion":2,"owner":"WrongOwner"}' 'invalid journal is preserved'
    }

    Invoke-Test 'invalid previous-only journal is parsed before movement and preserved in place' {
        $case = New-TestCase 'invalid-previous-only-journal'
        Set-TestArtifacts $case 'v1'
        New-Item -ItemType Directory -Force -Path $case.InstallRoot | Out-Null
        $previous = Join-Path $case.InstallRoot '.arcgis-pro-agent-journal-write-previous.bak'
        $current = Join-Path $case.InstallRoot '.arcgis-pro-agent-install-journal.json'
        [IO.File]::WriteAllText($previous, '{"schemaVersion":2,"owner":"WrongOwner","sentinel":"keep"}')
        $before = Get-Text $previous
        Assert-Throws { Invoke-TestInstall $case } 'journal|owner|schema|required|manual review' `
            'invalid previous-only journal must fail before movement'
        Assert-Equal (Get-Text $previous) $before 'invalid previous-only journal bytes remain in place'
        Assert-True (-not (Test-Path -LiteralPath $current)) 'invalid previous-only journal is not moved to current'
    }

    Invoke-Test 'mismatched current and previous journals are both preserved for manual review' {
        $case = New-TestCase 'mismatched-current-previous-journals'
        Set-TestArtifacts $case 'v1'
        Invoke-TestInstall $case
        Set-TestArtifacts $case 'v2'
        $child = Start-InstallerChild -Case $case -Mode 'crash-after-replace' `
            -ReadyPath (Join-Path $case.Base 'unused-ready') -ReleasePath (Join-Path $case.Base 'unused-release')
        Assert-True ($child.WaitForExit(30000)) 'journal mismatch fixture child exits promptly'
        Assert-Equal $child.ExitCode 91 'journal mismatch fixture crashes after Replace'
        $currentJournal = Register-CaseStageFromJournal $case
        $current = Join-Path $case.InstallRoot '.arcgis-pro-agent-install-journal.json'
        $previous = Join-Path $case.InstallRoot '.arcgis-pro-agent-journal-write-previous.bak'
        $otherTransactionId = [Guid]::NewGuid().ToString('N')
        $otherStage = Join-Path $tempRoot "ArcGISProAgent-install-$otherTransactionId"
        New-Item -ItemType Directory -Path $otherStage | Out-Null
        $script:otherTempPaths.Add($otherStage)
        $otherSentinel = Join-Path $otherStage 'mismatch-sentinel.txt'
        [IO.File]::WriteAllText($otherSentinel, 'keep')
        $currentJournal.transactionId = $otherTransactionId
        $currentJournal.stageRoot = $otherStage
        [IO.File]::WriteAllText($previous, ($currentJournal | ConvertTo-Json -Depth 20), (New-Object Text.UTF8Encoding($false)))
        $currentBefore = Get-Text $current
        $previousBefore = Get-Text $previous
        Assert-Throws { Invoke-TestInstall $case } 'transaction|mismatch|manual review|previous' `
            'mismatched current and previous journals must fail closed'
        Assert-Equal (Get-Text $current) $currentBefore 'current journal remains byte-for-byte unchanged'
        Assert-Equal (Get-Text $previous) $previousBefore 'mismatched previous journal remains byte-for-byte unchanged'
        Assert-Equal (Get-Text $otherSentinel) 'keep' 'mismatched previous stage sentinel is preserved'
    }

    Invoke-Test 'journal stageRoot for another transaction is rejected without touching its sentinel' {
        $case = New-TestCase 'cross-transaction-stage-root'
        Set-TestArtifacts $case 'v1'
        Invoke-TestInstall $case
        Set-TestArtifacts $case 'v2'
        $child = Start-InstallerChild -Case $case -Mode 'crash-after-replace' `
            -ReadyPath (Join-Path $case.Base 'unused-ready') -ReleasePath (Join-Path $case.Base 'unused-release')
        Assert-True ($child.WaitForExit(30000)) 'cross-transaction fixture child exits promptly'
        Assert-Equal $child.ExitCode 91 'cross-transaction fixture crashes after Replace'
        $journal = Register-CaseStageFromJournal $case
        $journalPath = Join-Path $case.InstallRoot '.arcgis-pro-agent-install-journal.json'
        $otherTransactionId = [Guid]::NewGuid().ToString('N')
        $otherStage = Join-Path $tempRoot "ArcGISProAgent-install-$otherTransactionId"
        New-Item -ItemType Directory -Path $otherStage | Out-Null
        $script:otherTempPaths.Add($otherStage)
        $sentinel = Join-Path $otherStage 'cross-transaction-sentinel.txt'
        [IO.File]::WriteAllText($sentinel, 'keep')
        $journal.stageRoot = $otherStage
        [IO.File]::WriteAllText($journalPath, ($journal | ConvertTo-Json -Depth 20), (New-Object Text.UTF8Encoding($false)))
        $journalBefore = Get-Text $journalPath
        Assert-Throws { Invoke-TestInstall $case } 'stageRoot|transaction|journal|manual review' `
            'journal must reject the exact stage directory of another transaction'
        Assert-Equal (Get-Text $journalPath) $journalBefore 'cross-transaction journal remains unchanged'
        Assert-Equal (Get-Text $sentinel) 'keep' 'cross-transaction stage sentinel remains untouched'
    }

    Invoke-Test 'journal stageRoot prefix extension is rejected without touching its sentinel' {
        $case = New-TestCase 'prefixed-stage-root'
        Set-TestArtifacts $case 'v1'
        Invoke-TestInstall $case
        Set-TestArtifacts $case 'v2'
        $child = Start-InstallerChild -Case $case -Mode 'crash-after-replace' `
            -ReadyPath (Join-Path $case.Base 'unused-ready') -ReleasePath (Join-Path $case.Base 'unused-release')
        Assert-True ($child.WaitForExit(30000)) 'prefix-stage fixture child exits promptly'
        Assert-Equal $child.ExitCode 91 'prefix-stage fixture crashes after Replace'
        $journal = Register-CaseStageFromJournal $case
        $journalPath = Join-Path $case.InstallRoot '.arcgis-pro-agent-install-journal.json'
        $invalidStage = Join-Path $tempRoot ("ArcGISProAgent-install-{0}-extra" -f [string]$journal.transactionId)
        New-Item -ItemType Directory -Path $invalidStage | Out-Null
        $sentinel = Join-Path $invalidStage 'prefix-sentinel.txt'
        [IO.File]::WriteAllText($sentinel, 'keep')
        try {
            $journal.stageRoot = $invalidStage
            [IO.File]::WriteAllText($journalPath, ($journal | ConvertTo-Json -Depth 20), (New-Object Text.UTF8Encoding($false)))
            $journalBefore = Get-Text $journalPath
            Assert-Throws { Invoke-TestInstall $case } 'stageRoot|transaction|journal|manual review' `
                'journal must reject a stage name with a suffix'
            Assert-Equal (Get-Text $journalPath) $journalBefore 'prefixed-stage journal remains unchanged'
            Assert-Equal (Get-Text $sentinel) 'keep' 'prefixed-stage sentinel remains untouched'
        } finally {
            $verifiedInvalid = [IO.Path]::GetFullPath($invalidStage).TrimEnd('\')
            Assert-NoTestReparseAncestor $verifiedInvalid
            Assert-Equal ([IO.Path]::GetFileName($verifiedInvalid)) `
                ("ArcGISProAgent-install-{0}-extra" -f [string]$journal.transactionId) `
                'invalid-stage cleanup uses the exact generated fixture name'
            $invalidParentEntity = [ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath((Split-Path -Parent $verifiedInvalid))
            $tempEntity = [ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath($tempRoot)
            Assert-True ($invalidParentEntity.Equals($tempEntity, [StringComparison]::OrdinalIgnoreCase)) `
                'invalid-stage cleanup fixture parent is exactly TEMP'
            if (Test-Path -LiteralPath $sentinel -PathType Leaf) { [IO.File]::Delete($sentinel) }
            if (Test-Path -LiteralPath $verifiedInvalid -PathType Container) { [IO.Directory]::Delete($verifiedInvalid, $false) }
        }
    }

    Invoke-Test 'journal transactionId must be a normalized lowercase N GUID' {
        $case = New-TestCase 'non-normalized-transaction-id'
        Set-TestArtifacts $case 'v1'
        Invoke-TestInstall $case
        Set-TestArtifacts $case 'v2'
        $child = Start-InstallerChild -Case $case -Mode 'crash-after-replace' `
            -ReadyPath (Join-Path $case.Base 'unused-ready') -ReleasePath (Join-Path $case.Base 'unused-release')
        Assert-True ($child.WaitForExit(30000)) 'non-normalized transaction fixture child exits promptly'
        Assert-Equal $child.ExitCode 91 'non-normalized transaction fixture crashes after Replace'
        $journal = Register-CaseStageFromJournal $case
        $journalPath = Join-Path $case.InstallRoot '.arcgis-pro-agent-install-journal.json'
        $journal.transactionId = ([string]$journal.transactionId).ToUpperInvariant()
        [IO.File]::WriteAllText($journalPath, ($journal | ConvertTo-Json -Depth 20), (New-Object Text.UTF8Encoding($false)))
        $journalBefore = Get-Text $journalPath
        Assert-Throws { Invoke-TestInstall $case } 'transactionId|normalized|journal|manual review' `
            'journal rejects an uppercase transactionId'
        Assert-Equal (Get-Text $journalPath) $journalBefore 'non-normalized journal remains unchanged'
    }

    Invoke-Test 'root and component junctions are rejected without touching external sentinels' {
        $rootCase = New-TestCase 'junction-root'
        Set-TestArtifacts $rootCase 'v1'
        $externalRoot = Join-Path $rootCase.Base 'external-root'
        New-Item -ItemType Directory -Force -Path $externalRoot | Out-Null
        $sentinel = Join-Path $externalRoot 'sentinel.txt'
        [IO.File]::WriteAllText($sentinel, 'keep')
        New-TestJunction -Path $rootCase.InstallRoot -Target $externalRoot
        Assert-Throws { Invoke-TestInstall $rootCase } 'reparse|junction|link' 'root junction rejected'
        Assert-Equal (Get-Text $sentinel) 'keep' 'root-junction sentinel preserved'
        Assert-True (-not (Test-Path -LiteralPath (Join-Path $externalRoot '0.1.0\mcp\Agent.dll'))) 'no junction escape write'

        $subCase = New-TestCase 'junction-component'
        Set-TestArtifacts $subCase 'v1'
        $versionDir = Join-Path $subCase.InstallRoot '0.1.0'
        New-Item -ItemType Directory -Force -Path $versionDir | Out-Null
        $externalComponent = Join-Path $subCase.Base 'external-component'
        New-Item -ItemType Directory -Force -Path $externalComponent | Out-Null
        $subSentinel = Join-Path $externalComponent 'sentinel.txt'
        [IO.File]::WriteAllText($subSentinel, 'keep')
        New-TestJunction -Path (Join-Path $versionDir 'mcp') -Target $externalComponent
        Assert-Throws { Invoke-TestInstall $subCase } 'reparse|junction|link' 'component junction rejected'
        Assert-Equal (Get-Text $subSentinel) 'keep' 'component-junction sentinel preserved'
        Assert-True (-not (Test-Path -LiteralPath (Join-Path $externalComponent 'Agent.dll'))) 'no component junction escape write'
    }

    Invoke-Test 'copy failure rolls back and next run succeeds' {
        $case = New-TestCase 'rollback-copy'
        Set-TestArtifacts $case 'v1'
        Invoke-TestInstall $case
        $manifestBefore = Get-Text $case.Manifest
        $target = (Get-TestRecords $case)[0].DestinationPath
        $targetBefore = Get-Text $target
        Set-TestArtifacts $case 'v2'
        Assert-Throws { Invoke-TestInstall -Case $case -FailurePoint 'Copy' } 'injected' 'copy failure injection'
        Assert-Equal (Get-Text $case.Manifest) $manifestBefore 'manifest rolled back after copy failure'
        Assert-Equal (Get-Text $target) $targetBefore 'file rolled back after copy failure'
        Assert-NoTransactionArtifacts $case
        Invoke-TestInstall $case
        Assert-Equal (Get-Text $target) 'mcp-v2' 'next run succeeds after copy rollback'
    }

    Invoke-Test 'stale failure rolls back and next run succeeds' {
        $case = New-TestCase 'rollback-stale'
        Set-TestArtifacts $case 'v1'
        Invoke-TestInstall $case
        $manifestBefore = Get-Text $case.Manifest
        $stale = (Get-TestRecords $case)[1].DestinationPath
        $reduced = Get-TestRecords $case -WithoutDesktop
        Assert-Throws { Invoke-TestInstall -Case $case -Records $reduced -FailurePoint 'Stale' } 'injected' 'stale failure injection'
        Assert-Equal (Get-Text $case.Manifest) $manifestBefore 'manifest rolled back after stale failure'
        Assert-True (Test-Path -LiteralPath $stale -PathType Leaf) 'stale restored after failure'
        Assert-NoTransactionArtifacts $case
        Invoke-TestInstall -Case $case -Records $reduced
        Assert-True (-not (Test-Path -LiteralPath $stale)) 'next run removes stale safely'
    }

    Invoke-Test 'manifest commit failure rolls back and next run succeeds' {
        $case = New-TestCase 'rollback-manifest'
        Set-TestArtifacts $case 'v1'
        Invoke-TestInstall $case
        $manifestBefore = Get-Text $case.Manifest
        $target = (Get-TestRecords $case)[0].DestinationPath
        $targetBefore = Get-Text $target
        Set-TestArtifacts $case 'v2'
        Assert-Throws { Invoke-TestInstall -Case $case -FailurePoint 'ManifestCommit' } 'injected' 'manifest failure injection'
        Assert-Equal (Get-Text $case.Manifest) $manifestBefore 'old manifest restored after commit failure'
        Assert-Equal (Get-Text $target) $targetBefore 'old file restored after manifest failure'
        Assert-NoTransactionArtifacts $case
        Invoke-TestInstall $case
        Assert-Equal (Get-Text $target) 'mcp-v2' 'next run succeeds after manifest rollback'
    }

    Write-Host "Installer tests passed: $script:passed" -ForegroundColor Green
} finally {
    foreach ($process in @($script:childProcesses | ForEach-Object { $_ })) {
        try {
            $process.Refresh()
            if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit(5000) | Out-Null }
        } catch { }
    }
    foreach ($externalTempPath in @($script:externalTempPaths | ForEach-Object { $_ })) {
        Remove-ExactTestStageTree $externalTempPath
    }
    foreach ($otherTempPath in @($script:otherTempPaths | ForEach-Object { $_ })) {
        Remove-ExactTestStageTree $otherTempPath
    }
    if ($null -ne $script:substDrive) {
        & subst.exe $script:substDrive /d | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "Unable to remove test SUBST drive: $script:substDrive" }
        $script:substDrive = $null
    }
    foreach ($junction in @($script:junctions | Sort-Object { $_.Length } -Descending)) {
        if (Test-Path -LiteralPath $junction) {
            $item = Get-Item -LiteralPath $junction -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) {
                throw "Refusing to clean non-junction test path: $junction"
            }
            [IO.Directory]::Delete($junction)
        }
    }
    $verifiedTestRoot = [IO.Path]::GetFullPath($testRoot).TrimEnd('\')
    if (-not $verifiedTestRoot.StartsWith($tempRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to clean unsafe installer test root: $verifiedTestRoot"
    }
    if (Test-Path -LiteralPath $verifiedTestRoot) {
        Remove-Item -LiteralPath $verifiedTestRoot -Recurse -Force
    }
}
