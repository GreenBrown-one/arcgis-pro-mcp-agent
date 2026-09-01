param([string]$Candidate)

$locations = @(
    $Candidate,
    $env:ARCGIS_PRO_HOME,
    (Get-ItemProperty -Path 'HKLM:\SOFTWARE\ESRI\ArcGISPro' -ErrorAction SilentlyContinue).InstallDir,
    'C:\Program Files\ArcGIS\Pro'
) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }

foreach ($location in $locations) {
    $full = [IO.Path]::GetFullPath($location).TrimEnd('\')
    if (Test-Path -LiteralPath (Join-Path $full 'bin\ArcGIS.Core.dll')) {
        $full
        exit 0
    }
}

throw 'ArcGIS Pro SDK assemblies were not found. Set ARCGIS_PRO_HOME or pass -Candidate.'
