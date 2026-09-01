[CmdletBinding()]
param(
    [string]$ArcGISProInstallDir,
    [string]$WorkspaceRoot,
    [switch]$SourceAssertionsOnly
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

function Assert-NoTextMatch {
    param(
        [Parameter(Mandatory = $true)][string[]]$Paths,
        [Parameter(Mandatory = $true)][string]$Pattern,
        [Parameter(Mandatory = $true)][string]$FailureMessage
    )

    $files = @($Paths | ForEach-Object {
        if (Test-Path -LiteralPath $_ -PathType Container) {
            Get-ChildItem -LiteralPath $_ -Recurse -File |
                Where-Object { $_.FullName -notmatch '[\\/](bin|obj|node_modules|dist|target)[\\/]' }
        } elseif (Test-Path -LiteralPath $_ -PathType Leaf) {
            Get-Item -LiteralPath $_
        }
    })

    $matches = @($files | Select-String -Pattern $Pattern -ErrorAction Stop)
    if ($matches.Count -gt 0) {
        $locations = ($matches | ForEach-Object { "$($_.Path):$($_.LineNumber)" }) -join ', '
        throw "$FailureMessage Matches: $locations"
    }
}

function Assert-ExactSet {
    param(
        [Parameter(Mandatory = $true)][string[]]$Actual,
        [Parameter(Mandatory = $true)][string[]]$Expected,
        [Parameter(Mandatory = $true)][string]$FailureMessage
    )

    $actualDistinct = @($Actual | Sort-Object -Unique)
    $expectedDistinct = @($Expected | Sort-Object -Unique)
    $differences = @(Compare-Object -ReferenceObject $expectedDistinct -DifferenceObject $actualDistinct)
    if ($Actual.Count -ne $expectedDistinct.Count -or
        $actualDistinct.Count -ne $expectedDistinct.Count -or
        $differences.Count -ne 0) {
        throw "$FailureMessage Expected: $($expectedDistinct -join ', '). Found: $($actualDistinct -join ', ')"
    }
}

function Invoke-Task3SourceAssertions {
    param([Parameter(Mandatory = $true)][string]$Root)

    $mcpSourceFiles = @(Get-ChildItem -LiteralPath (Join-Path $Root 'src\ArcGISProAgent.Mcp') -Recurse -Filter '*.cs' -File |
        Where-Object { $_.FullName -notmatch '[\\/](bin|obj)[\\/]' })
    $compiledToolSource = ($mcpSourceFiles | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw }) -join "`n"
    $toolNames = @([regex]::Matches($compiledToolSource, 'McpServerTool\s*\(\s*Name\s*=\s*"([^"]+)"') |
        ForEach-Object { $_.Groups[1].Value })
    $expectedToolNames = @(
        'arcgis_connection_status',
        'arcgis_capabilities',
        'arcgis_describe_context',
        'arcgis_list_layers',
        'arcgis_describe_layer',
        'arcgis_list_fields',
        'arcgis_count_features',
        'arcgis_query_features',
        'arcgis_query_spatial',
        'arcgis_get_selection',
        'arcgis_select_by_attribute',
        'arcgis_select_by_location',
        'arcgis_clear_selection',
        'arcgis_activate_view',
        'arcgis_zoom_to_layer',
        'arcgis_zoom_to_extent',
        'arcgis_flash_features'
    )
    Assert-ExactSet -Actual $toolNames -Expected $expectedToolNames `
        -FailureMessage 'The Phase-2 R1 MCP host tool allowlist is incorrect.'

    $toolAttributes = @([regex]::Matches(
        $compiledToolSource,
        'McpServerTool\s*\((?<body>[\s\S]*?)\)\]') |
        ForEach-Object { $_.Groups['body'].Value })
    if ($toolAttributes.Count -ne 17 -or
        @($toolAttributes | Where-Object { $_ -notmatch 'Destructive\s*=\s*false' }).Count -ne 0) {
        throw 'All 17 compiled MCP tool declarations must explicitly set Destructive=false.'
    }

    $phaseGuide = Join-Path $Root 'docs\development\phase-2-user-guide.md'
    $phaseGuideText = Get-Content -LiteralPath $phaseGuide -Raw
    $documentedBlock = [regex]::Match(
        $phaseGuideText,
        '<!-- phase-2-tools:start -->(?<body>[\s\S]*?)<!-- phase-2-tools:end -->')
    if (-not $documentedBlock.Success) {
        throw 'The Phase-2 guide must contain the bounded public-tool table markers.'
    }
    $documentedToolNames = @([regex]::Matches(
        $documentedBlock.Groups['body'].Value,
        '\|\s*R[01]\s*\|\s*`(arcgis_[a-z0-9_]+)`') |
        ForEach-Object { $_.Groups[1].Value })
    Assert-ExactSet -Actual $documentedToolNames -Expected $expectedToolNames `
        -FailureMessage 'The documented Phase-2 public tool set does not match the compiled allowlist.'

    foreach ($legacyToolName in @('Ping', 'Echo')) {
        if ($toolNames -contains $legacyToolName) {
            throw "Legacy MCP tool remains public: $legacyToolName"
        }
    }
    if (@($toolNames | Where-Object {
        $_ -match '^(?:pro\.)' -or $_ -match '(?i)(?:sql|operation|dispatch)' -or $_ -match 'R[23]'
    }).Count -ne 0) {
        throw 'The public MCP allowlist contains a legacy, arbitrary, or R2/R3 tool name.'
    }

    Assert-NoTextMatch -Paths @($mcpSourceFiles.FullName) `
        -Pattern '(?i)\b(?:rawSql|whereClause|rawWkt|rawCim|scriptText|genericDispatcher)\b' `
        -FailureMessage 'The public MCP surface must not expose raw SQL/WKT/CIM/script or a generic dispatcher.'

    $dispatcherPath = Join-Path $Root 'src\ArcGISProAgent.AddIn\ArcGisOperationDispatcher.cs'
    $dispatcherSource = Get-Content -LiteralPath $dispatcherPath -Raw
    $expectedOperationIds = @(
        'connection.health',
        'context.describe',
        'layers.list',
        'layers.describe',
        'layers.fields',
        'query.feature_count',
        'query.features',
        'query.spatial',
        'selection.describe',
        'selection.by_attribute',
        'selection.by_location',
        'selection.clear',
        'map_view.activate',
        'map_view.zoom_to_layer',
        'map_view.zoom_to_extent',
        'map_view.flash_features'
    )

    $runtimeIdsBlock = [regex]::Match(
        $dispatcherSource,
        'RuntimeOperationIds\s*=\s*Array\.AsReadOnly\s*\(\s*\[(?<body>[\s\S]*?)\]\s*\)',
        [Text.RegularExpressions.RegexOptions]::CultureInvariant)
    if (-not $runtimeIdsBlock.Success) {
        throw 'The Add-In must declare an immutable RuntimeOperationIds allowlist.'
    }
    $runtimeIds = @([regex]::Matches($runtimeIdsBlock.Groups['body'].Value, '"([^"]+)"') |
        ForEach-Object { $_.Groups[1].Value })
    Assert-ExactSet -Actual $runtimeIds -Expected $expectedOperationIds `
        -FailureMessage 'The Add-In runtime capability allowlist is incorrect.'

    $dispatcherCases = @([regex]::Matches(
        $dispatcherSource,
        '"(connection\.health|context\.describe|layers\.list|layers\.describe|layers\.fields|query\.feature_count|query\.features|query\.spatial|selection\.describe|selection\.by_attribute|selection\.by_location|selection\.clear|map_view\.activate|map_view\.zoom_to_layer|map_view\.zoom_to_extent|map_view\.flash_features)"(?:\s+when\s+[^=]+)?\s*=>') |
        ForEach-Object { $_.Groups[1].Value })
    Assert-ExactSet -Actual $dispatcherCases -Expected $expectedOperationIds `
        -FailureMessage 'The Add-In dispatcher R1 case allowlist is incorrect.'

    if ($dispatcherSource -match 'RiskLevel\.R[23]' -or
        $dispatcherSource -match '"(?:edit|project|geoprocessing|export|filesystem)\.[^"]+"') {
        throw 'The Add-In runtime must not advertise an R2/R3 operation.'
    }

    $querySource = Get-Content -LiteralPath (Join-Path $Root 'src\ArcGISProAgent.AddIn\Operations\QueryOperations.cs') -Raw
    $requiredQuerySafeguards = @(
        @('ValidatePredicateCompatibility\s*\(', 'Typed predicates must be validated against FieldType before scanning.'),
        @('FieldType\.Geometry\s+or\s+FieldType\.Blob\s+or\s+FieldType\.Raster\s+or\s+FieldType\.XML', 'Unsupported predicate field types must fail closed.'),
        @('GeometryEngine\.Instance\.Project\s*\(', 'Every spatial source must be projected to the known target spatial reference.'),
        @('MaximumPublicResultBytes\s*=\s*900\s*\*\s*1024', 'Feature-record construction must enforce the 900 KiB public result budget.'),
        @('<unsupported:non-finite-number>', 'Non-finite floating-point values must be converted before JSON serialization.')
    )
    foreach ($safeguard in $requiredQuerySafeguards) {
        if ($querySource -notmatch $safeguard[0]) {
            throw $safeguard[1]
        }
    }

    $reviewSafeguardFailures = @()
    $sourceGeometryMethodIndex = $querySource.IndexOf(
        'private static Geometry? ReadSourceLayerGeometry',
        [StringComparison]::Ordinal)
    $sourceDefinitionCheckIndex = if ($sourceGeometryMethodIndex -ge 0) {
        $querySource.IndexOf(
            'RequireKnownSourceSpatialReference(featureDefinition)',
            $sourceGeometryMethodIndex,
            [StringComparison]::Ordinal)
    } else { -1 }
    $sourceCursorIndex = if ($sourceGeometryMethodIndex -ge 0) {
        $querySource.IndexOf(
            'using var cursor = table.Search(filter, false)',
            $sourceGeometryMethodIndex,
            [StringComparison]::Ordinal)
    } else { -1 }
    if ($sourceGeometryMethodIndex -lt 0 -or
        $sourceDefinitionCheckIndex -lt $sourceGeometryMethodIndex -or
        $sourceCursorIndex -lt $sourceDefinitionCheckIndex -or
        $querySource -notmatch 'RequireKnownSourceSpatialReference[\s\S]*?GetSpatialReference\(\)[\s\S]*?IsUnknown') {
        $reviewSafeguardFailures += 'Source feature-class spatial references must be validated before scanning.'
    }

    $pageObjectIdsIndex = $querySource.IndexOf(
        'internal static IReadOnlyList<long> ReadPageObjectIds',
        [StringComparison]::Ordinal)
    $publicRecordsIndex = $querySource.IndexOf(
        'private static IReadOnlyList<FeatureRecord> ReadPublicRecords',
        [StringComparison]::Ordinal)
    $featureRecordIndex = $querySource.IndexOf(
        'new FeatureRecord',
        [StringComparison]::Ordinal)
    $resultBudgetIndex = $querySource.IndexOf(
        'serializedResultBytes',
        [StringComparison]::Ordinal)
    if ($pageObjectIdsIndex -lt 0 -or
        $publicRecordsIndex -lt $pageObjectIdsIndex -or
        $featureRecordIndex -lt $publicRecordsIndex -or
        $resultBudgetIndex -lt $publicRecordsIndex -or
        $querySource -notmatch 'filter\.ObjectIDs\s*=\s*publicObjectIds' -or
        $querySource -notmatch 'pageObjectIds\.Count\s*>\s*limit') {
        $reviewSafeguardFailures += 'Query paging must use a two-phase OID/public-record path and charge only public records.'
    }

    foreach ($helperSignature in @(
        'internal static Field ResolvePredicateField',
        'internal static Table OpenTable'
    )) {
        if ($querySource.IndexOf($helperSignature, [StringComparison]::Ordinal) -lt 0) {
            $reviewSafeguardFailures += 'Task-4 safe query helper visibility seams are missing.'
        }
    }

    if ($reviewSafeguardFailures.Count -gt 0) {
        throw ($reviewSafeguardFailures -join ' ')
    }

    if ($querySource -match '\(long\)offset\s*\+\s*limit\s*\+\s*1\s*>\s*MaximumFallbackRows') {
        throw 'Fallback paging must prove end-of-data before returning an empty high-offset page.'
    }

    if ($dispatcherSource -notmatch 'MaximumBridgeResponseBytes\s*=\s*1024\s*\*\s*1024' -or
        $dispatcherSource -notmatch 'JsonSerializer\.SerializeToUtf8Bytes\s*\(\s*response\s*,\s*BridgeJson\.Options\s*\)') {
        throw 'The dispatcher must enforce the exact 1 MiB serialized BridgeResponse frame limit.'
    }

    $task4ReviewFailures = @()
    $selectionSource = Get-Content -LiteralPath (Join-Path $Root 'src\ArcGISProAgent.AddIn\Operations\SelectionOperations.cs') -Raw
    $emptySelectionCalls = @([regex]::Matches(
        $selectionSource,
        'HandleEmptySelection\s*\(\s*arguments\.LayerUri\s*,\s*layer\s*,\s*arguments\.Mode\s*\)'))
    if ($emptySelectionCalls.Count -ne 2 -or
        $selectionSource -notmatch 'mode\s+is\s+SelectionCombinationMode\.Replace[\s\S]*?layer\.ClearSelection\s*\(\s*\)[\s\S]*?ReadFinalSelection\s*\(' -or
        $selectionSource -match 'ObjectIDs\s*=\s*Array\.Empty<long>\s*\(\s*\)') {
        $task4ReviewFailures += 'Empty attribute/spatial matches must clear only Replace, no-op Add/Remove/Toggle, and return the actual count without Select(empty ObjectIDs).'
    }

    $mapViewSource = Get-Content -LiteralPath (Join-Path $Root 'src\ArcGISProAgent.AddIn\Operations\MapViewOperations.cs') -Raw
    if ($mapViewSource -notmatch 'FrameworkApplication\.Panes[\s\S]*?\.OfType<Pane>\s*\(\s*\)' -or
        $mapViewSource -notmatch 'IMapPane[\s\S]*?MapView\.Map\.URI' -or
        $mapViewSource -notmatch 'ILayoutPane[\s\S]*?LayoutView\.Layout\.URI' -or
        $mapViewSource -match '\.GetMapPanes\s*\(' -or
        $mapViewSource -match '\.FindLayoutPanes\s*\(') {
        $task4ReviewFailures += 'Existing panes must be enumerated from FrameworkApplication.Panes and matched through documented IMapPane/ILayoutPane view URIs.'
    }

    if ($task4ReviewFailures.Count -gt 0) {
        throw ($task4ReviewFailures -join ' ')
    }
}

if ([string]::IsNullOrWhiteSpace($WorkspaceRoot)) {
    $WorkspaceRoot = Split-Path -Parent $PSScriptRoot
}
$repoRoot = [IO.Path]::GetFullPath($WorkspaceRoot)
$resolver = Join-Path $repoRoot 'scripts\Resolve-ArcGISProInstall.ps1'
$installer = Join-Path $repoRoot 'scripts\Install-Dev.ps1'
$installerCore = Join-Path $repoRoot 'scripts\Install-Dev.Core.psm1'
$installerTests = Join-Path $repoRoot 'scripts\Test-InstallDev.ps1'
$solution = Join-Path $repoRoot 'McpServer.sln'
$desktopRoot = Join-Path $repoRoot 'apps\desktop'
$tauriManifest = Join-Path $desktopRoot 'src-tauri\Cargo.toml'
$foundationGuide = Join-Path $repoRoot 'docs\development\foundation.md'
$phaseGuide = Join-Path $repoRoot 'docs\development\phase-2-user-guide.md'
$smokeChecklist = Join-Path $repoRoot 'docs\development\phase-2-smoke.md'
$foundationSmoke = Join-Path $repoRoot 'docs\development\foundation-smoke-pending.md'
$noRegisterFolder = Join-Path ([IO.Path]::GetTempPath()) 'ArcGISProAgent-foundation-no-register'
if (Test-Path -LiteralPath $noRegisterFolder) {
    throw "The no-register guard path must not exist: $noRegisterFolder"
}

foreach ($requiredPath in @($resolver, $installer, $installerCore, $installerTests, $solution, $desktopRoot, $tauriManifest, $foundationGuide, $phaseGuide, $smokeChecklist, $foundationSmoke)) {
    if (-not (Test-Path -LiteralPath $requiredPath)) {
        throw "Required foundation artifact is missing: $requiredPath"
    }
}

Invoke-Task3SourceAssertions -Root $repoRoot
if ($SourceAssertionsOnly) {
    Write-Host 'Task 3 source assertions passed.' -ForegroundColor Green
    return
}

if ([string]::IsNullOrWhiteSpace($ArcGISProInstallDir)) {
    $ArcGISProInstallDir = & $resolver -Candidate 'D:\arcgis_pro'
}
$ArcGISProInstallDir = [IO.Path]::GetFullPath($ArcGISProInstallDir).TrimEnd('\')

Invoke-CheckedCommand -Command 'powershell' -Arguments @(
    '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $installerTests,
    '-WorkspaceRoot', $repoRoot
) -FailureMessage 'Development installer safety tests failed'

Invoke-CheckedCommand -Command 'dotnet' -Arguments @(
    'test', $solution, '--configuration', 'Release',
    "-p:ArcGISProInstallDir=$ArcGISProInstallDir", "-p:ArcGISFolder=$noRegisterFolder"
) -FailureMessage '.NET foundation tests failed'

Push-Location $desktopRoot
try {
    Invoke-CheckedCommand -Command 'npm.cmd' -Arguments @('test') -FailureMessage 'Frontend tests failed'
    Invoke-CheckedCommand -Command 'npm.cmd' -Arguments @('run', 'build') -FailureMessage 'Frontend build failed'
    Invoke-CheckedCommand -Command 'npm.cmd' -Arguments @(
        'run', 'tauri', '--', 'build', '--debug', '--no-bundle'
    ) -FailureMessage 'Tauri debug build failed'
} finally {
    Pop-Location
}

Invoke-CheckedCommand -Command 'cargo' -Arguments @(
    'test', '--manifest-path', $tauriManifest
) -FailureMessage 'Rust tests failed'

Assert-NoTextMatch -Paths @((Join-Path $repoRoot 'src\ArcGISProAgent.AddIn')) `
    -Pattern 'C:\\Program Files\\ArcGIS\\Pro|file:///C:/Program%20Files/ArcGIS/Pro' `
    -FailureMessage 'The new Add-In must not hard-code the default ArcGIS Pro installation path.'

Assert-NoTextMatch -Paths @((Join-Path $desktopRoot 'src')) `
    -Pattern '(?i)api[ _-]?key' `
    -FailureMessage 'The subscription-only UI must not expose an API-key input.'

$commandSource = Get-Content -LiteralPath (Join-Path $desktopRoot 'src-tauri\src\commands.rs') -Raw
$clientSource = Get-Content -LiteralPath (Join-Path $desktopRoot 'src-tauri\src\codex\client.rs') -Raw
$productionCommandSource = $commandSource -replace '(?s)\n#\[cfg\(test\)\].*$', ''
if ($productionCommandSource -notmatch '"type"\s*:\s*"chatgpt"' -or
    $productionCommandSource -notmatch 'Some\("apiKey"\)\s*\|\s*Some\(_\)\s*=>\s*AccountSnapshot::UnsupportedAuth') {
    throw 'Login must request ChatGPT and explicitly reject API-key or unknown account types.'
}
foreach ($secretEnvironmentName in @('OPENAI_API_KEY', 'AZURE_OPENAI_API_KEY', 'CODEX_API_KEY')) {
    $removalExpression = [regex]::Escape('env_remove("' + $secretEnvironmentName + '")')
    if ($clientSource -notmatch $removalExpression) {
        throw "Codex startup must remove inherited secret environment variable: $secretEnvironmentName"
    }
}
if ($productionCommandSource -match '"type"\s*:\s*"apiKey"') {
    throw 'The desktop must never start an API-key login flow.'
}

$expectedVersion = '0.2.0-preview.1'
$projectVersionFiles = @(
    (Join-Path $repoRoot 'src\ArcGISProAgent.Mcp\ArcGISProAgent.Mcp.csproj'),
    (Join-Path $repoRoot 'src\ArcGISProAgent.AddIn\ArcGISProAgent.AddIn.csproj')
)
foreach ($versionFile in $projectVersionFiles) {
    [xml]$projectXml = Get-Content -LiteralPath $versionFile -Raw
    $declaredVersions = @($projectXml.Project.PropertyGroup.Version | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($declaredVersions.Count -ne 1 -or [string]$declaredVersions[0] -ne $expectedVersion) {
        throw "Project version must be exactly $expectedVersion in $versionFile"
    }
}
$jsonVersionScript = 'require(process.argv[1]).version'
foreach ($jsonVersionFile in @(
    (Join-Path $desktopRoot 'package.json'),
    (Join-Path $desktopRoot 'package-lock.json'),
    (Join-Path $desktopRoot 'src-tauri\tauri.conf.json')
)) {
    $jsonVersion = & node -p $jsonVersionScript ([IO.Path]::GetFullPath($jsonVersionFile))
    if ($LASTEXITCODE -ne 0) {
        throw "Unable to read the structured JSON version from $jsonVersionFile"
    }
    if ([string]$jsonVersion -ne $expectedVersion) {
        throw "$jsonVersionFile version must be exactly $expectedVersion"
    }
}
$cargoMetadataText = & cargo metadata --no-deps --format-version 1 --manifest-path $tauriManifest
if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed (exit code $LASTEXITCODE)" }
$cargoMetadata = $cargoMetadataText | ConvertFrom-Json
$desktopPackage = @($cargoMetadata.packages | Where-Object { $_.name -eq 'arcgis-pro-agent-desktop' })
if ($desktopPackage.Count -ne 1 -or [string]$desktopPackage[0].version -ne $expectedVersion) {
    throw "Cargo package version must be exactly $expectedVersion"
}

$legacyFiles = @(
    (Join-Path $repoRoot 'McpServer\ArcGisMcpServer'),
    (Join-Path $repoRoot 'AddIn\APBridgeAddIn')
) | ForEach-Object {
    if (Test-Path -LiteralPath $_ -PathType Container) {
        Get-ChildItem -LiteralPath $_ -Recurse -File |
            Where-Object { $_.FullName -notmatch '[\\/](bin|obj)[\\/]' }
    }
}
if (@($legacyFiles).Count -gt 0) {
    throw "Superseded sample files remain in the working tree: $($legacyFiles.FullName -join ', ')"
}

$trackedFiles = @(& git -c "safe.directory=$repoRoot" -C $repoRoot ls-files)
if ($LASTEXITCODE -ne 0) {
    throw "Unable to inspect tracked files (exit code $LASTEXITCODE)"
}
$forbiddenTracked = @($trackedFiles | Where-Object {
    $_ -match '(^|/)(bin|obj|node_modules|dist|target)(/|$)' -or
    $_ -match '(^|/)(install-manifest\.json|arcgis-pro-agent-runtime\.json|bridge\.json)$' -or
    $_ -match '(^|/)runtime(/|$)'
})
if ($forbiddenTracked.Count -gt 0) {
    throw "Generated output or a runtime secret is tracked: $($forbiddenTracked -join ', ')"
}
$trackedJsonWithSecrets = @($trackedFiles | Where-Object { $_ -match '\.json$' } | ForEach-Object {
    $trackedJsonPath = Join-Path $repoRoot $_
    if ((Get-Content -LiteralPath $trackedJsonPath -Raw) -match '(?i)"(?:token|authToken)"\s*:') { $_ }
})
if ($trackedJsonWithSecrets.Count -gt 0) {
    throw "A tracked JSON file contains a runtime token/authToken field: $($trackedJsonWithSecrets -join ', ')"
}

$smokeText = Get-Content -LiteralPath $smokeChecklist -Raw
if ($smokeText -notmatch '(?i)pending') {
    throw 'The manual GUI smoke checklist must remain explicitly pending until it is actually performed.'
}

Write-Host 'Foundation non-GUI verification passed.' -ForegroundColor Green
