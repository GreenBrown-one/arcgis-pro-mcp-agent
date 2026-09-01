Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$script:ManifestOwner = 'ArcGISProAgent'
$script:ManifestSchemaVersion = 1
$script:AddInId = '{1A0481EA-3F43-4C98-B4B5-A58C727CD115}'
$script:AddInPackageFileName = 'ArcGISProAgent.AddIn.esriAddinX'
$script:ForbiddenDataExtensions = @(
    '.aprx', '.ppkx', '.mapx', '.lyrx', '.gdb', '.sde', '.geodatabase', '.mdb',
    '.shp', '.shx', '.dbf', '.prj', '.cpg', '.sbn', '.sbx',
    '.tif', '.tiff', '.img', '.jp2', '.jpg', '.jpeg', '.png',
    '.csv', '.kml', '.kmz', '.geojson', '.pdf', '.svg'
)
$script:AllowedExtensions = @{
    mcp = @('.dll', '.exe', '.json', '.pdb')
    desktop = @('.exe')
    addin = @('.esriaddinx')
}

function Get-ArcGISProAgentDefaultAddInRoot {
    Get-CanonicalInstallPath (Join-Path $env:USERPROFILE "Documents\ArcGIS\AddIns\ArcGISPro\$script:AddInId")
}

function Get-ArcGISProAgentLegacyDefaultAddInRoot {
    Get-CanonicalInstallPath (Join-Path $env:USERPROFILE 'Documents\ArcGIS\AddIns\ArcGISProAgent')
}

if (-not ('ArcGISProAgentInstaller.WindowsFileSystem' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace ArcGISProAgentInstaller
{
    public static class WindowsFileSystem
    {
        private const uint InvalidFileAttributes = 0xffffffff;
        private const uint FileFlagBackupSemantics = 0x02000000;
        private const uint OpenExisting = 3;
        private const uint FileShareRead = 1;
        private const uint FileShareWrite = 2;
        private const uint FileShareDelete = 4;
        private const uint FileNameNormalized = 0;
        private const uint VolumeNameGuid = 1;
        private const int ErrorFileNotFound = 2;
        private const int ErrorPathNotFound = 3;

        [StructLayout(LayoutKind.Sequential)]
        private struct ByHandleFileInformation
        {
            public uint FileAttributes;
            public System.Runtime.InteropServices.ComTypes.FILETIME CreationTime;
            public System.Runtime.InteropServices.ComTypes.FILETIME LastAccessTime;
            public System.Runtime.InteropServices.ComTypes.FILETIME LastWriteTime;
            public uint VolumeSerialNumber;
            public uint FileSizeHigh;
            public uint FileSizeLow;
            public uint NumberOfLinks;
            public uint FileIndexHigh;
            public uint FileIndexLow;
        }

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern uint GetFileAttributesW(string fileName);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern SafeFileHandle CreateFileW(
            string fileName,
            uint desiredAccess,
            uint shareMode,
            IntPtr securityAttributes,
            uint creationDisposition,
            uint flagsAndAttributes,
            IntPtr templateFile);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern uint GetFinalPathNameByHandleW(
            SafeFileHandle file,
            System.Text.StringBuilder path,
            uint pathLength,
            uint flags);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool GetFileInformationByHandle(
            SafeFileHandle file,
            out ByHandleFileInformation information);

        private static bool Exists(string path)
        {
            uint attributes = GetFileAttributesW(path);
            if (attributes != InvalidFileAttributes) return true;
            int error = Marshal.GetLastWin32Error();
            if (error == ErrorFileNotFound || error == ErrorPathNotFound) return false;
            throw new Win32Exception(error, "Unable to inspect Windows path: " + path);
        }

        private static string ReadFinalPath(SafeFileHandle handle, uint flags)
        {
            uint capacity = 512;
            while (true)
            {
                var buffer = new System.Text.StringBuilder((int)capacity);
                uint length = GetFinalPathNameByHandleW(handle, buffer, capacity, flags);
                if (length == 0)
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Unable to resolve Windows entity path.");
                if (length < capacity) return buffer.ToString();
                capacity = length + 1;
            }
        }

        private static string NormalizeFinalPath(string path)
        {
            if (path.StartsWith(@"\\?\UNC\", StringComparison.OrdinalIgnoreCase))
                path = @"\\" + path.Substring(8);
            else if (path.StartsWith(@"\\?\", StringComparison.OrdinalIgnoreCase) &&
                     !path.StartsWith(@"\\?\Volume{", StringComparison.OrdinalIgnoreCase))
                path = path.Substring(4);
            return path.Length > 1 ? path.TrimEnd('\\') : path;
        }

        public static string GetEntityPath(string inputPath)
        {
            if (String.IsNullOrWhiteSpace(inputPath)) throw new ArgumentException("Path is empty.", "inputPath");
            string full = Path.GetFullPath(Environment.ExpandEnvironmentVariables(inputPath));
            var missing = new List<string>();
            string existing = full;
            while (!Exists(existing))
            {
                string trimmed = existing.TrimEnd(Path.DirectorySeparatorChar);
                string name = Path.GetFileName(trimmed);
                string parent = Path.GetDirectoryName(trimmed);
                if (String.IsNullOrEmpty(name) || String.IsNullOrEmpty(parent) || parent == existing)
                    throw new IOException("No resolvable existing ancestor for Windows path: " + full);
                missing.Insert(0, name);
                existing = parent;
            }
            if (missing.Count > 0 && File.Exists(existing))
                throw new IOException("A missing path tail follows an existing file: " + existing);

            using (SafeFileHandle handle = CreateFileW(
                existing,
                0,
                FileShareRead | FileShareWrite | FileShareDelete,
                IntPtr.Zero,
                OpenExisting,
                FileFlagBackupSemantics,
                IntPtr.Zero))
            {
                if (handle.IsInvalid)
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Unable to open Windows path entity: " + existing);
                string finalPath;
                try { finalPath = ReadFinalPath(handle, FileNameNormalized | VolumeNameGuid); }
                catch (Win32Exception) { finalPath = ReadFinalPath(handle, FileNameNormalized); }
                finalPath = NormalizeFinalPath(finalPath);
                foreach (string segment in missing)
                    finalPath = finalPath.TrimEnd('\\') + "\\" + segment;
                return finalPath;
            }
        }

        public static string GetFileIdentity(string inputPath)
        {
            string full = Path.GetFullPath(Environment.ExpandEnvironmentVariables(inputPath));
            using (SafeFileHandle handle = CreateFileW(
                full,
                0,
                FileShareRead | FileShareWrite | FileShareDelete,
                IntPtr.Zero,
                OpenExisting,
                FileFlagBackupSemantics,
                IntPtr.Zero))
            {
                if (handle.IsInvalid)
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Unable to open Windows file identity: " + full);
                ByHandleFileInformation information;
                if (!GetFileInformationByHandle(handle, out information))
                    throw new Win32Exception(Marshal.GetLastWin32Error(), "Unable to read Windows file identity: " + full);
                ulong fileIndex = ((ulong)information.FileIndexHigh << 32) | information.FileIndexLow;
                return information.VolumeSerialNumber.ToString("X8") + ":" + fileIndex.ToString("X16");
            }
        }
    }
}
'@
}

function Get-CanonicalInstallPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        throw 'A required path is empty.'
    }
    $full = [IO.Path]::GetFullPath([Environment]::ExpandEnvironmentVariables($Path))
    $root = [IO.Path]::GetPathRoot($full)
    if ($full.Length -gt $root.Length) {
        $full = $full.TrimEnd('\')
    }
    $full
}

function Test-InstallPathWithin {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Root
    )

    try {
        $candidate = [ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath((Get-CanonicalInstallPath $Path))
        $boundary = [ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath((Get-CanonicalInstallPath $Root))
    } catch {
        throw "Unable to resolve a Windows entity path; refusing the operation: $($_.Exception.Message)"
    }
    $candidate.Equals($boundary, [StringComparison]::OrdinalIgnoreCase) -or
        $candidate.StartsWith($boundary.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)
}

function Assert-FullyQualifiedCanonicalPath {
    param([Parameter(Mandatory = $true)][string]$Path, [string]$Name = 'path')

    $canonical = Get-CanonicalInstallPath $Path
    if ([string]::IsNullOrWhiteSpace([IO.Path]::GetPathRoot($Path)) -or
        -not $Path.Equals($canonical, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$Name must be a fully qualified canonical path: $Path"
    }
    $canonical
}

function Assert-NoGisDataAncestor {
    param([Parameter(Mandatory = $true)][string]$Path)

    $canonical = Get-CanonicalInstallPath $Path
    $root = [IO.Path]::GetPathRoot($canonical)
    $relative = $canonical.Substring($root.Length)
    foreach ($segment in @($relative.Split(@('\'), [StringSplitOptions]::RemoveEmptyEntries))) {
        $extension = [IO.Path]::GetExtension($segment).ToLowerInvariant()
        if ($script:ForbiddenDataExtensions -contains $extension) {
            throw "Refusing a GIS data, sidecar, or export container in path: $canonical"
        }
    }
}

function Assert-NoReparsePointInPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    $canonical = Get-CanonicalInstallPath $Path
    $root = [IO.Path]::GetPathRoot($canonical)
    $current = $root
    $relative = $canonical.Substring($root.Length)
    foreach ($segment in @($relative.Split(@('\'), [StringSplitOptions]::RemoveEmptyEntries))) {
        $current = Join-Path $current $segment
        if (Test-Path -LiteralPath $current) {
            $item = Get-Item -LiteralPath $current -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Refusing a reparse point, junction, or symbolic link in path: $current"
            }
        }
    }
}

function Assert-SafePathForRoots {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string[]]$AllowedRoots
    )

    $canonical = Get-CanonicalInstallPath $Path
    $inside = $false
    foreach ($root in $AllowedRoots) {
        if (Test-InstallPathWithin -Path $canonical -Root $root) {
            $inside = $true
            break
        }
    }
    if (-not $inside) {
        throw "Path escapes the allowed installation roots: $canonical"
    }
    Assert-NoGisDataAncestor $canonical
    Assert-NoReparsePointInPath $canonical
    $canonical
}

function Assert-SafeSourceFile {
    param([Parameter(Mandatory = $true)][string]$Path)

    $canonical = Get-CanonicalInstallPath $Path
    Assert-NoGisDataAncestor $canonical
    Assert-NoReparsePointInPath $canonical
    if (-not (Test-Path -LiteralPath $canonical -PathType Leaf)) {
        throw "Source artifact is missing or not a file: $canonical"
    }
    $canonical
}

function Ensure-SafeDirectory {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string[]]$AllowedRoots
    )

    $canonical = Assert-SafePathForRoots -Path $Path -AllowedRoots $AllowedRoots
    $selectedBoundary = $null
    foreach ($allowedRoot in $AllowedRoots) {
        if (Test-InstallPathWithin -Path $canonical -Root $allowedRoot) {
            $selectedBoundary = Get-CanonicalInstallPath $allowedRoot
            break
        }
    }
    if ($null -eq $selectedBoundary) { throw "No allowed directory boundary for: $canonical" }
    $root = [IO.Path]::GetPathRoot($canonical)
    $current = $root
    $relative = $canonical.Substring($root.Length)
    foreach ($segment in @($relative.Split(@('\'), [StringSplitOptions]::RemoveEmptyEntries))) {
        $next = Join-Path $current $segment
        Assert-NoGisDataAncestor $next
        Assert-NoReparsePointInPath $next
        if (Test-Path -LiteralPath $next) {
            if (-not (Test-Path -LiteralPath $next -PathType Container)) {
                throw "A required installation directory is a file: $next"
            }
        } else {
            if (-not (Test-InstallPathWithin -Path $next -Root $selectedBoundary)) {
                throw "Refusing to create an ancestor outside the allowed root: $next"
            }
            Assert-NoReparsePointInPath $current
            [IO.Directory]::CreateDirectory($next) | Out-Null
        }
        Assert-NoReparsePointInPath $next
        $current = $next
    }
    $canonical
}

function Initialize-AuthorizedRoot {
    param(
        [Parameter(Mandatory = $true)][string]$RootPath,
        [Parameter(Mandatory = $true)][ValidateSet('InstallRoot','AddInRoot')][string]$RootName,
        [scriptblock]$OperationHook
    )

    $authorizedRoot = Assert-FullyQualifiedCanonicalPath $RootPath $RootName
    $volumeRoot = [IO.Path]::GetPathRoot($authorizedRoot)
    if ($authorizedRoot.Equals($volumeRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "$RootName must not be a drive or share root: $authorizedRoot"
    }
    Assert-NoGisDataAncestor $authorizedRoot
    Assert-NoReparsePointInPath $authorizedRoot
    try {
        $authorizedEntity = [ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath($authorizedRoot)
    } catch {
        throw "Unable to resolve the authorized $RootName entity path: $($_.Exception.Message)"
    }

    $missingSegments = New-Object System.Collections.Generic.List[string]
    $existingAncestor = $authorizedRoot
    while (-not (Test-Path -LiteralPath $existingAncestor)) {
        $trimmed = $existingAncestor.TrimEnd('\')
        $segment = [IO.Path]::GetFileName($trimmed)
        $parent = [IO.Path]::GetDirectoryName($trimmed)
        if ([string]::IsNullOrWhiteSpace($segment) -or [string]::IsNullOrWhiteSpace($parent) -or
            $parent.Equals($existingAncestor, [StringComparison]::OrdinalIgnoreCase)) {
            throw "No existing ancestor can authorize $RootName creation: $authorizedRoot"
        }
        $missingSegments.Insert(0, $segment)
        $existingAncestor = Get-CanonicalInstallPath $parent
    }
    if (-not (Test-Path -LiteralPath $existingAncestor -PathType Container)) {
        throw "The deepest existing ancestor for $RootName is not a directory: $existingAncestor"
    }
    Assert-NoGisDataAncestor $existingAncestor
    Assert-NoReparsePointInPath $existingAncestor
    try {
        $currentEntity = [ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath($existingAncestor)
    } catch {
        throw "Unable to resolve the existing ancestor for ${RootName}: $($_.Exception.Message)"
    }

    $current = $existingAncestor
    foreach ($segment in $missingSegments) {
        Assert-NoGisDataAncestor $current
        Assert-NoReparsePointInPath $current
        if (-not (Test-Path -LiteralPath $current -PathType Container)) {
            throw "The authorized $RootName ancestor changed before creation: $current"
        }
        $observedCurrentEntity = [ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath($current)
        if (-not $observedCurrentEntity.Equals($currentEntity, [StringComparison]::OrdinalIgnoreCase)) {
            throw "The authorized $RootName ancestor entity changed during creation: $current"
        }
        $observedRootPrediction = [ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath($authorizedRoot)
        if (-not $observedRootPrediction.Equals($authorizedEntity, [StringComparison]::OrdinalIgnoreCase)) {
            throw "The authorized $RootName entity prediction changed during creation: $authorizedRoot"
        }

        $next = Get-CanonicalInstallPath (Join-Path $current $segment)
        if (Test-Path -LiteralPath $next) {
            throw "An authorized $RootName tail segment appeared concurrently: $next"
        }
        Assert-NoGisDataAncestor $next
        Assert-NoReparsePointInPath $next
        $expectedNextEntity = $currentEntity.TrimEnd('\') + '\' + $segment
        $predictedNextEntity = [ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath($next)
        if (-not $predictedNextEntity.Equals($expectedNextEntity, [StringComparison]::OrdinalIgnoreCase)) {
            throw "The authorized $RootName tail entity prediction is unsafe: $next"
        }
        if ($null -ne $OperationHook) {
            & $OperationHook 'BeforeAuthorizedRootSegmentCreate' ([PSCustomObject]@{
                Root=$authorizedRoot; RootName=$RootName; Path=$next; Parent=$current
            })
        }
        Assert-NoReparsePointInPath $current
        if (-not ([ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath($current)).Equals(
            $currentEntity, [StringComparison]::OrdinalIgnoreCase)) {
            throw "The authorized $RootName ancestor entity changed immediately before creation: $current"
        }
        if (Test-Path -LiteralPath $next) {
            throw "An authorized $RootName tail segment appeared immediately before creation: $next"
        }
        [IO.Directory]::CreateDirectory($next) | Out-Null
        if ($null -ne $OperationHook) {
            & $OperationHook 'AfterAuthorizedRootSegmentCreate' ([PSCustomObject]@{
                Root=$authorizedRoot; RootName=$RootName; Path=$next; Parent=$current
            })
        }
        Assert-NoGisDataAncestor $next
        Assert-NoReparsePointInPath $next
        if (-not (Test-Path -LiteralPath $next -PathType Container)) {
            throw "The authorized $RootName tail segment is not a directory after creation: $next"
        }
        $actualNextEntity = [ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath($next)
        if (-not $actualNextEntity.Equals($expectedNextEntity, [StringComparison]::OrdinalIgnoreCase)) {
            throw "The authorized $RootName tail entity changed after creation: $next"
        }
        $current = $next
        $currentEntity = $actualNextEntity
    }
    if (-not $current.Equals($authorizedRoot, [StringComparison]::OrdinalIgnoreCase) -or
        -not $currentEntity.Equals($authorizedEntity, [StringComparison]::OrdinalIgnoreCase)) {
        throw "The initialized $RootName does not match its authorized path and entity: $authorizedRoot"
    }
    Assert-NoGisDataAncestor $authorizedRoot
    Assert-NoReparsePointInPath $authorizedRoot
    $authorizedRoot
}

function Assert-AllowedComponentPath {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Component,
        [Parameter(Mandatory = $true)][string]$Version,
        [Parameter(Mandatory = $true)][string]$InstallRoot,
        [Parameter(Mandatory = $true)][string]$AddInRoot,
        [switch]$AllowLegacyVersionedAddIn
    )

    $componentKey = $Component.ToLowerInvariant()
    if (-not $script:AllowedExtensions.ContainsKey($componentKey)) {
        throw "Unknown install component: $Component"
    }
    $canonical = Get-CanonicalInstallPath $Path
    $extension = [IO.Path]::GetExtension($canonical).ToLowerInvariant()
    if ($script:AllowedExtensions[$componentKey] -notcontains $extension) {
        throw "Component '$componentKey' does not allow output extension '$extension': $canonical"
    }
    switch ($componentKey) {
        'mcp' { $componentRoot = Join-Path $InstallRoot "$Version\mcp" }
        'desktop' { $componentRoot = Join-Path $InstallRoot "$Version\desktop" }
        'addin' {
            $expected = Get-CanonicalInstallPath (Join-Path $AddInRoot $script:AddInPackageFileName)
            $legacyExpected = Get-CanonicalInstallPath (Join-Path $AddInRoot "$Version\$script:AddInPackageFileName")
            $isExactLegacy = $AllowLegacyVersionedAddIn -and
                (Get-CanonicalInstallPath $AddInRoot).Equals(
                    (Get-ArcGISProAgentLegacyDefaultAddInRoot), [StringComparison]::OrdinalIgnoreCase) -and
                $canonical.Equals($legacyExpected, [StringComparison]::OrdinalIgnoreCase)
            if (-not $canonical.Equals($expected, [StringComparison]::OrdinalIgnoreCase) -and -not $isExactLegacy) {
                throw "Add-In output must be the fixed package directly below AddInRoot: $canonical"
            }
            Assert-NoGisDataAncestor $canonical
            return $canonical
        }
    }
    if (-not (Test-InstallPathWithin -Path $canonical -Root $componentRoot) -or
        $canonical.Equals((Get-CanonicalInstallPath $componentRoot), [StringComparison]::OrdinalIgnoreCase)) {
        throw "Component output is outside its versioned component root: $canonical"
    }
    Assert-NoGisDataAncestor $canonical
    $canonical
}

function Assert-SafeInstallTopology {
    param(
        [Parameter(Mandatory = $true)][string]$SourceRoot,
        [Parameter(Mandatory = $true)][string]$InstallRoot,
        [Parameter(Mandatory = $true)][string]$AddInRoot
    )

    $source = Get-CanonicalInstallPath $SourceRoot
    $install = Get-CanonicalInstallPath $InstallRoot
    $addin = Get-CanonicalInstallPath $AddInRoot
    foreach ($namedRoot in @(
        [PSCustomObject]@{ Name = 'SourceRoot'; Path = $source },
        [PSCustomObject]@{ Name = 'InstallRoot'; Path = $install },
        [PSCustomObject]@{ Name = 'AddInRoot'; Path = $addin }
    )) {
        $driveRoot = [IO.Path]::GetPathRoot($namedRoot.Path)
        if ($namedRoot.Path.Equals($driveRoot, [StringComparison]::OrdinalIgnoreCase)) {
            throw "$($namedRoot.Name) must not be a drive root: $($namedRoot.Path)"
        }
        Assert-NoGisDataAncestor $namedRoot.Path
        Assert-NoReparsePointInPath $namedRoot.Path
    }
    if ((Test-InstallPathWithin -Path $install -Root $addin) -or
        (Test-InstallPathWithin -Path $addin -Root $install)) {
        throw 'InstallRoot and AddInRoot overlap.'
    }
    foreach ($destination in @($install, $addin)) {
        if ((Test-InstallPathWithin -Path $destination -Root $source) -or
            (Test-InstallPathWithin -Path $source -Root $destination)) {
            throw "Installation roots and source root must not contain one another: $destination"
        }
    }
    [PSCustomObject]@{ SourceRoot = $source; InstallRoot = $install; AddInRoot = $addin }
}

function Test-ArcGISProAgentInstallTopology {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$SourceRoot,
        [Parameter(Mandatory = $true)][string]$InstallRoot,
        [Parameter(Mandatory = $true)][string]$AddInRoot
    )

    Assert-SafeInstallTopology -SourceRoot $SourceRoot -InstallRoot $InstallRoot -AddInRoot $AddInRoot
}

function Get-RequiredProperty {
    param([object]$Object, [string]$Name, [string]$Context)
    if ($null -eq $Object -or $Object.PSObject.Properties.Name -notcontains $Name) {
        throw "Manifest $Context is missing required property '$Name'."
    }
    $Object.$Name
}

function Read-StrictInstallManifest {
    param(
        [string]$ManifestPath,
        [string]$Version,
        [string]$InstallRoot,
        [string]$AddInRoot,
        [string]$PreviousAddInRootWithoutEntries
    )

    $allowedRoots = @($InstallRoot, $AddInRoot)
    $allowedPreviousRoot = $null
    if (-not [string]::IsNullOrWhiteSpace($PreviousAddInRootWithoutEntries)) {
        $allowedPreviousRoot = Get-CanonicalInstallPath $PreviousAddInRootWithoutEntries
        if (-not (Get-CanonicalInstallPath $AddInRoot).Equals(
                (Get-ArcGISProAgentDefaultAddInRoot), [StringComparison]::OrdinalIgnoreCase) -or
            -not $allowedPreviousRoot.Equals(
                (Get-ArcGISProAgentLegacyDefaultAddInRoot), [StringComparison]::OrdinalIgnoreCase)) {
            throw 'The previous Add-In root exception is limited to the exact legacy and official default roots.'
        }
        $allowedRoots += $allowedPreviousRoot
    }
    Assert-SafePathForRoots -Path $ManifestPath -AllowedRoots $allowedRoots | Out-Null
    if (-not (Test-Path -LiteralPath $ManifestPath)) { return $null }
    if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
        throw "Manifest path is not a file: $ManifestPath"
    }
    try {
        $manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
    } catch {
        throw "Manifest JSON is invalid and was preserved: $($_.Exception.Message)"
    }

    $schemaVersion = Get-RequiredProperty $manifest 'schemaVersion' 'root'
    $owner = Get-RequiredProperty $manifest 'owner' 'root'
    $manifestVersion = Get-RequiredProperty $manifest 'version' 'root'
    $declaredManifestPath = [string](Get-RequiredProperty $manifest 'manifestPath' 'root')
    $declaredInstallRoot = [string](Get-RequiredProperty $manifest 'installRoot' 'root')
    $declaredAddInRoot = [string](Get-RequiredProperty $manifest 'addInRoot' 'root')
    $files = Get-RequiredProperty $manifest 'files' 'root'
    if ($schemaVersion -ne $script:ManifestSchemaVersion -or $schemaVersion -is [string]) {
        throw "Manifest schemaVersion must be integer $($script:ManifestSchemaVersion)."
    }
    if ($owner -ne $script:ManifestOwner -or $manifestVersion -ne $Version) {
        throw 'Manifest owner or version is invalid.'
    }
    $declaredManifestPath = Assert-FullyQualifiedCanonicalPath $declaredManifestPath 'manifestPath'
    $declaredInstallRoot = Assert-FullyQualifiedCanonicalPath $declaredInstallRoot 'installRoot'
    $declaredAddInRoot = Assert-FullyQualifiedCanonicalPath $declaredAddInRoot 'addInRoot'
    $addInRootMatches = $declaredAddInRoot.Equals($AddInRoot, [StringComparison]::OrdinalIgnoreCase)
    $allowedEmptyRootMismatch = $null -ne $allowedPreviousRoot -and
        $declaredAddInRoot.Equals($allowedPreviousRoot, [StringComparison]::OrdinalIgnoreCase)
    if (-not $declaredManifestPath.Equals($ManifestPath, [StringComparison]::OrdinalIgnoreCase) -or
        -not $declaredInstallRoot.Equals($InstallRoot, [StringComparison]::OrdinalIgnoreCase) -or
        (-not $addInRootMatches -and -not $allowedEmptyRootMismatch)) {
        throw 'Manifest path or roots do not match the requested installation roots.'
    }

    $entries = New-Object System.Collections.Generic.List[object]
    $seen = @{}
    foreach ($entry in @($files)) {
        $entryOwner = Get-RequiredProperty $entry 'owner' 'entry'
        $entryVersion = Get-RequiredProperty $entry 'version' 'entry'
        $component = [string](Get-RequiredProperty $entry 'component' 'entry')
        $path = [string](Get-RequiredProperty $entry 'path' 'entry')
        $sha256 = [string](Get-RequiredProperty $entry 'sha256' 'entry')
        $lengthValue = Get-RequiredProperty $entry 'length' 'entry'
        if ($entryOwner -ne $script:ManifestOwner -or $entryVersion -ne $Version) {
            throw "Manifest entry owner or version is invalid: $path"
        }
        $path = Assert-FullyQualifiedCanonicalPath $path 'entry path'
        $path = Assert-AllowedComponentPath -Path $path -Component $component -Version $Version `
            -InstallRoot $InstallRoot -AddInRoot $declaredAddInRoot `
            -AllowLegacyVersionedAddIn:($declaredAddInRoot.Equals(
                (Get-ArcGISProAgentLegacyDefaultAddInRoot), [StringComparison]::OrdinalIgnoreCase))
        Assert-SafePathForRoots -Path $path -AllowedRoots $allowedRoots | Out-Null
        if ($sha256 -notmatch '^[0-9A-Fa-f]{64}$') {
            throw "Manifest entry hash is invalid: $path"
        }
        if (($lengthValue -isnot [int]) -and ($lengthValue -isnot [long])) {
            throw "Manifest entry length must be an integer: $path"
        }
        $length = [long]$lengthValue
        if ($length -lt 0) { throw "Manifest entry length is negative: $path" }
        $key = $path.ToLowerInvariant()
        if ($seen.ContainsKey($key)) { throw "Manifest contains duplicate path: $path" }
        $seen[$key] = $true
        $entries.Add([PSCustomObject]@{
            owner = $script:ManifestOwner
            version = $Version
            component = $component.ToLowerInvariant()
            path = $path
            sha256 = $sha256.ToLowerInvariant()
            length = $length
        })
    }
    if ($allowedEmptyRootMismatch -and @($entries | Where-Object { $_.component -eq 'addin' }).Count -ne 0) {
        throw 'Manifest Add-In root mismatch is allowed only after every legacy Add-In entry is removed.'
    }
    [PSCustomObject]@{ Raw = $manifest; Entries = @($entries | ForEach-Object { $_ }) }
}

function Assert-ExistingOwnedFileMatches {
    param([object]$Entry, [string[]]$AllowedRoots)

    $path = Assert-SafePathForRoots -Path $Entry.path -AllowedRoots $AllowedRoots
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Manifest-owned file is missing or was rebuilt: $path"
    }
    Assert-NoReparsePointInPath $path
    $item = Get-Item -LiteralPath $path -Force
    if ([long]$item.Length -ne [long]$Entry.length) {
        throw "Manifest-owned file length changed; preserving it and failing: $path"
    }
    $hash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $Entry.sha256) {
        throw "Manifest-owned file hash changed; preserving it and failing: $path"
    }
}

function Remove-SafeFile {
    param([string]$Path, [string[]]$AllowedRoots)
    $safe = Assert-SafePathForRoots -Path $Path -AllowedRoots $AllowedRoots
    if (Test-Path -LiteralPath $safe) {
        if (-not (Test-Path -LiteralPath $safe -PathType Leaf)) {
            throw "Refusing to remove a non-file: $safe"
        }
        Assert-SafePathForRoots -Path $safe -AllowedRoots $AllowedRoots | Out-Null
        [IO.File]::Delete($safe)
    }
}

function Remove-SafeTree {
    param([string]$Root, [string]$Boundary)

    $rootPath = Assert-SafePathForRoots -Path $Root -AllowedRoots @($Boundary)
    if (-not (Test-Path -LiteralPath $rootPath)) { return }
    $directories = New-Object System.Collections.Generic.List[string]
    $pending = New-Object System.Collections.Generic.Stack[string]
    $pending.Push($rootPath)
    while ($pending.Count -gt 0) {
        $directory = $pending.Pop()
        Assert-SafePathForRoots -Path $directory -AllowedRoots @($Boundary) | Out-Null
        $directories.Add($directory)
        foreach ($child in @(Get-ChildItem -LiteralPath $directory -Force)) {
            if (($child.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Refusing to clean a staged reparse point: $($child.FullName)"
            }
            if ($child.PSIsContainer) {
                $pending.Push($child.FullName)
            } else {
                Remove-SafeFile -Path $child.FullName -AllowedRoots @($Boundary)
            }
        }
    }
    foreach ($directory in @($directories | Sort-Object { $_.Length } -Descending)) {
        Assert-SafePathForRoots -Path $directory -AllowedRoots @($Boundary) | Out-Null
        [IO.Directory]::Delete($directory, $false)
    }
}

function Get-InstallerFileState {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][string[]]$AllowedRoots)

    $safe = Assert-SafePathForRoots -Path $Path -AllowedRoots $AllowedRoots
    if (-not (Test-Path -LiteralPath $safe -PathType Leaf)) {
        throw "Expected transaction file is missing: $safe"
    }
    Assert-NoReparsePointInPath $safe
    $item = Get-Item -LiteralPath $safe -Force
    $identity = try {
        [ArcGISProAgentInstaller.WindowsFileSystem]::GetFileIdentity($safe)
    } catch {
        throw "Unable to resolve Windows file identity; refusing the operation: $safe. $($_.Exception.Message)"
    }
    [PSCustomObject]@{
        sha256 = (Get-FileHash -LiteralPath $safe -Algorithm SHA256).Hash.ToLowerInvariant()
        length = [long]$item.Length
        identity = [string]$identity
    }
}

function Test-InstallerFileState {
    param(
        [string]$Path,
        [string]$Sha256,
        [long]$Length,
        [string]$Identity,
        [string[]]$AllowedRoots
    )
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    $actual = Get-InstallerFileState -Path $Path -AllowedRoots $AllowedRoots
    if ($actual.sha256 -ne $Sha256 -or [long]$actual.length -ne $Length) { return $false }
    if (-not [string]::IsNullOrWhiteSpace($Identity) -and $actual.identity -ne $Identity) { return $false }
    $true
}

function Assert-InstallerFileState {
    param(
        [string]$Path,
        [string]$Sha256,
        [long]$Length,
        [string]$Identity,
        [string[]]$AllowedRoots,
        [string]$Context
    )
    if (-not (Test-InstallerFileState -Path $Path -Sha256 $Sha256 -Length $Length `
        -Identity $Identity -AllowedRoots $AllowedRoots)) {
        throw "$Context hash, length, or Windows file identity changed: $Path"
    }
}

function New-InstallerSiblingPath {
    param(
        [string]$Target,
        [string]$Kind,
        [string]$OperationId,
        [string[]]$AllowedRoots
    )
    $parent = Split-Path -Parent $Target
    $name = ".arcgis-pro-agent-$Kind-$OperationId-" + [IO.Path]::GetFileName($Target)
    $path = Get-CanonicalInstallPath (Join-Path $parent $name)
    Assert-SafePathForRoots -Path $path -AllowedRoots $AllowedRoots | Out-Null
    if (Test-Path -LiteralPath $path) { throw "Transaction path already exists: $path" }
    $path
}

function Assert-ExactJournalProperties {
    param([object]$Object, [string[]]$Expected, [string]$Context)
    if ($null -eq $Object) { throw "Transaction journal $Context is null." }
    $actual = @($Object.PSObject.Properties.Name | Sort-Object)
    $wanted = @($Expected | Sort-Object)
    if ($actual.Count -ne $wanted.Count -or @(Compare-Object $actual $wanted).Count -ne 0) {
        throw "Transaction journal $Context has an invalid property set."
    }
}

function Assert-ExactInstallerStageRoot {
    param([string]$StageRoot, [string]$TransactionId)

    if ($TransactionId -cnotmatch '^[0-9a-f]{32}$' -or
        [Guid]::ParseExact($TransactionId, 'N').ToString('N') -cne $TransactionId) {
        throw 'Transaction journal transactionId must be a normalized lowercase N GUID.'
    }
    $canonical = Assert-FullyQualifiedCanonicalPath $StageRoot 'stageRoot'
    $expectedName = "ArcGISProAgent-install-$TransactionId"
    if ([IO.Path]::GetFileName($canonical) -cne $expectedName) {
        throw "Transaction journal stageRoot does not match its transactionId: $canonical"
    }
    $stageBase = Get-CanonicalInstallPath ([IO.Path]::GetTempPath())
    try {
        $entityParent = [ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath((Split-Path -Parent $canonical))
        $entityStageBase = [ArcGISProAgentInstaller.WindowsFileSystem]::GetEntityPath($stageBase)
    } catch {
        throw "Unable to resolve the transaction stageRoot entity; manual review is required: $($_.Exception.Message)"
    }
    if (-not $entityParent.Equals($entityStageBase, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Transaction journal stageRoot parent is not exactly the system TEMP entity: $canonical"
    }
    Assert-NoGisDataAncestor $canonical
    Assert-NoReparsePointInPath $canonical
    $canonical
}

function Write-InstallerJournal {
    param([object]$Journal, [string]$JournalPath, [string[]]$AllowedRoots)

    $text = $Journal | ConvertTo-Json -Depth 8
    $parent = Ensure-SafeDirectory -Path (Split-Path -Parent $JournalPath) -AllowedRoots $AllowedRoots
    $writeId = [Guid]::NewGuid().ToString('N')
    $temporary = Get-CanonicalInstallPath (Join-Path $parent ".arcgis-pro-agent-journal-write-$writeId.tmp")
    $previous = Get-CanonicalInstallPath (Join-Path $parent '.arcgis-pro-agent-journal-write-previous.bak')
    foreach ($path in @($temporary, $previous, $JournalPath)) {
        Assert-SafePathForRoots -Path $path -AllowedRoots $AllowedRoots | Out-Null
    }
    if (Test-Path -LiteralPath $temporary) { throw "Journal temporary path already exists: $temporary" }
    if (Test-Path -LiteralPath $previous) { throw "A previous atomic journal backup requires recovery: $previous" }
    $encoding = New-Object Text.UTF8Encoding($false)
    $bytes = $encoding.GetBytes($text)
    $stream = New-Object IO.FileStream(
        $temporary,
        [IO.FileMode]::CreateNew,
        [IO.FileAccess]::Write,
        [IO.FileShare]::None,
        4096,
        [IO.FileOptions]::WriteThrough)
    try {
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
    } finally {
        $stream.Dispose()
    }
    if (Test-Path -LiteralPath $JournalPath -PathType Leaf) {
        Assert-SafePathForRoots -Path $JournalPath -AllowedRoots $AllowedRoots | Out-Null
        [IO.File]::Replace($temporary, $JournalPath, $previous, $true)
        if ([IO.File]::ReadAllText($JournalPath) -ne $text) {
            throw "Atomic transaction journal verification failed: $JournalPath"
        }
        Remove-SafeFile -Path $previous -AllowedRoots $AllowedRoots
    } elseif (Test-Path -LiteralPath $JournalPath) {
        throw "Transaction journal target is not a file: $JournalPath"
    } else {
        [IO.File]::Move($temporary, $JournalPath)
    }
}

function New-InstallerJournalOperation {
    param(
        [string]$Kind,
        [string]$Component,
        [string]$Target,
        [string]$Backup,
        [string]$Temporary,
        [object]$OldState,
        [object]$NewState
    )
    [PSCustomObject][ordered]@{
        id = [Guid]::NewGuid().ToString('N')
        kind = $Kind
        component = $Component
        target = $Target
        backup = $Backup
        temporary = $Temporary
        rollback = ''
        applied = $false
        oldSha256 = if ($null -ne $OldState) { [string]$OldState.sha256 } else { '' }
        oldLength = if ($null -ne $OldState) { [long]$OldState.length } else { [long]-1 }
        oldIdentity = if ($null -ne $OldState) { [string]$OldState.identity } else { '' }
        newSha256 = if ($null -ne $NewState) { [string]$NewState.sha256 } else { '' }
        newLength = if ($null -ne $NewState) { [long]$NewState.length } else { [long]-1 }
        tempIdentity = if ($null -ne $NewState) { [string]$NewState.identity } else { '' }
        backupSha256 = ''
        backupLength = [long]-1
        backupIdentity = ''
    }
}

function Read-StrictInstallerJournal {
    param(
        [string]$JournalPath,
        [string]$SourceRoot,
        [string]$InstallRoot,
        [string]$AddInRoot,
        [string]$Version
    )
    if (-not (Test-Path -LiteralPath $JournalPath)) { return $null }
    if (-not (Test-Path -LiteralPath $JournalPath -PathType Leaf)) {
        throw "Transaction journal is not a file: $JournalPath"
    }
    $allowedRoots = @($InstallRoot, $AddInRoot)
    Assert-SafePathForRoots -Path $JournalPath -AllowedRoots $allowedRoots | Out-Null
    try { $raw = Get-Content -LiteralPath $JournalPath -Raw | ConvertFrom-Json }
    catch { throw "Transaction journal JSON is invalid and was preserved: $($_.Exception.Message)" }
    $rootProperties = @(
        'schemaVersion','owner','version','transactionId','phase','sourceRoot','installRoot',
        'addInRoot','manifestPath','stageRoot','operations'
    )
    Assert-ExactJournalProperties -Object $raw -Expected $rootProperties -Context 'root'
    if ($raw.schemaVersion -ne 2 -or $raw.schemaVersion -is [string]) { throw 'Transaction journal schemaVersion must be integer 2.' }
    if ([string]$raw.owner -ne $script:ManifestOwner) { throw 'Transaction journal owner is invalid.' }
    if ([string]$raw.version -ne $Version) { throw 'Transaction journal version is invalid.' }
    $transactionId = [string]$raw.transactionId
    if ($transactionId -cnotmatch '^[0-9a-f]{32}$' -or
        [Guid]::ParseExact($transactionId, 'N').ToString('N') -cne $transactionId) {
        throw 'Transaction journal transactionId must be a normalized lowercase N GUID.'
    }
    if ([string]$raw.phase -notin @('applying','committed-cleanup-pending')) { throw 'Transaction journal phase is invalid.' }
    foreach ($rootPair in @(
        [PSCustomObject]@{ Name='sourceRoot'; Actual=[string]$raw.sourceRoot; Expected=$SourceRoot },
        [PSCustomObject]@{ Name='installRoot'; Actual=[string]$raw.installRoot; Expected=$InstallRoot },
        [PSCustomObject]@{ Name='addInRoot'; Actual=[string]$raw.addInRoot; Expected=$AddInRoot },
        [PSCustomObject]@{ Name='manifestPath'; Actual=[string]$raw.manifestPath; Expected=(Join-Path $InstallRoot 'install-manifest.json') }
    )) {
        $canonical = Assert-FullyQualifiedCanonicalPath $rootPair.Actual $rootPair.Name
        if (-not $canonical.Equals((Get-CanonicalInstallPath $rootPair.Expected), [StringComparison]::OrdinalIgnoreCase)) {
            throw "Transaction journal $($rootPair.Name) does not match the requested installation."
        }
    }
    $stageRoot = Assert-ExactInstallerStageRoot -StageRoot ([string]$raw.stageRoot) -TransactionId $transactionId

    $operationProperties = @(
        'id','kind','component','target','backup','temporary','rollback','applied',
        'oldSha256','oldLength','oldIdentity','newSha256','newLength','tempIdentity',
        'backupSha256','backupLength','backupIdentity'
    )
    $operations = New-Object System.Collections.Generic.List[object]
    foreach ($operation in @($raw.operations)) {
        Assert-ExactJournalProperties -Object $operation -Expected $operationProperties -Context 'operation'
        if ([string]$operation.id -notmatch '^[0-9a-fA-F]{32}$') { throw 'Transaction journal operation id is invalid.' }
        if ([string]$operation.kind -notin @('Replace','New','Stale','ManifestReplace','ManifestNew')) {
            throw 'Transaction journal operation kind is invalid.'
        }
        if ($operation.applied -isnot [bool]) { throw 'Transaction journal operation applied must be Boolean.' }
        $target = Assert-FullyQualifiedCanonicalPath ([string]$operation.target) 'operation target'
        Assert-SafePathForRoots -Path $target -AllowedRoots $allowedRoots | Out-Null
        $component = [string]$operation.component
        if ($component -eq 'manifest') {
            if ([string]$operation.kind -notin @('ManifestReplace','ManifestNew') -or
                -not $target.Equals((Get-CanonicalInstallPath (Join-Path $InstallRoot 'install-manifest.json')), [StringComparison]::OrdinalIgnoreCase)) {
                throw 'Transaction journal manifest operation is invalid.'
            }
        } else {
            Assert-AllowedComponentPath -Path $target -Component $component -Version $Version `
                -InstallRoot $InstallRoot -AddInRoot $AddInRoot `
                -AllowLegacyVersionedAddIn:((Get-CanonicalInstallPath $AddInRoot).Equals(
                    (Get-ArcGISProAgentLegacyDefaultAddInRoot), [StringComparison]::OrdinalIgnoreCase)) | Out-Null
        }
        foreach ($pathProperty in @('backup','temporary','rollback')) {
            $value = [string]$operation.$pathProperty
            if ([string]::IsNullOrWhiteSpace($value)) { continue }
            $value = Assert-FullyQualifiedCanonicalPath $value "operation $pathProperty"
            Assert-SafePathForRoots -Path $value -AllowedRoots $allowedRoots | Out-Null
            if (-not (Get-CanonicalInstallPath (Split-Path -Parent $value)).Equals(
                (Get-CanonicalInstallPath (Split-Path -Parent $target)), [StringComparison]::OrdinalIgnoreCase)) {
                throw "Transaction journal $pathProperty is not a target sibling."
            }
            if ([IO.Path]::GetFileName($value) -notlike ".arcgis-pro-agent-$pathProperty-$($operation.id)-*") {
                $expectedKind = if ($pathProperty -eq 'temporary') { 'new' } else { $pathProperty }
                if ([IO.Path]::GetFileName($value) -notlike ".arcgis-pro-agent-$expectedKind-$($operation.id)-*") {
                    throw "Transaction journal $pathProperty name is invalid."
                }
            }
        }
        foreach ($hashName in @('oldSha256','newSha256','backupSha256')) {
            $value = [string]$operation.$hashName
            if (-not [string]::IsNullOrWhiteSpace($value) -and $value -notmatch '^[0-9a-f]{64}$') {
                throw "Transaction journal $hashName is invalid."
            }
        }
        foreach ($identityName in @('oldIdentity','tempIdentity','backupIdentity')) {
            $value = [string]$operation.$identityName
            if (-not [string]::IsNullOrWhiteSpace($value) -and $value -notmatch '^[0-9A-F]{8}:[0-9A-F]{16}$') {
                throw "Transaction journal $identityName is invalid."
            }
        }
        foreach ($lengthName in @('oldLength','newLength','backupLength')) {
            if (($operation.$lengthName -isnot [int]) -and ($operation.$lengthName -isnot [long])) {
                throw "Transaction journal $lengthName must be an integer."
            }
        }
        $operations.Add($operation)
    }
    $raw.operations = $operations
    $raw
}

function Read-StableStrictInstallerJournal {
    param(
        [string]$JournalPath,
        [string]$SourceRoot,
        [string]$InstallRoot,
        [string]$AddInRoot,
        [string]$Version
    )
    if (-not (Test-Path -LiteralPath $JournalPath)) { return $null }
    $allowedRoots = @($InstallRoot, $AddInRoot)
    try {
        $before = Get-InstallerFileState -Path $JournalPath -AllowedRoots $allowedRoots
        $journal = Read-StrictInstallerJournal -JournalPath $JournalPath -SourceRoot $SourceRoot `
            -InstallRoot $InstallRoot -AddInRoot $AddInRoot -Version $Version
        $after = Get-InstallerFileState -Path $JournalPath -AllowedRoots $allowedRoots
        if ($before.sha256 -ne $after.sha256 -or [long]$before.length -ne [long]$after.length -or
            $before.identity -ne $after.identity) {
            throw 'identity, hash, or length changed during strict parsing'
        }
        [PSCustomObject]@{ Journal=$journal; State=$after }
    } catch {
        throw "Transaction journal requires manual review and was preserved at '$JournalPath': $($_.Exception.Message)"
    }
}

function Assert-StableJournalFileState {
    param([string]$JournalPath, [object]$ExpectedState, [string[]]$AllowedRoots)
    Assert-InstallerFileState -Path $JournalPath -Sha256 $ExpectedState.sha256 -Length $ExpectedState.length `
        -Identity $ExpectedState.identity -AllowedRoots $AllowedRoots `
        -Context 'Transaction journal changed after strict parsing; manual review is required and the file was preserved'
}

function Get-JournalOperationJson {
    param([object]$Operation)
    $Operation | ConvertTo-Json -Depth 5 -Compress
}

function Assert-CompatibleAtomicJournalPair {
    param([object]$Current, [object]$Previous)

    if ([string]$Current.transactionId -cne [string]$Previous.transactionId) {
        throw 'Current and previous transaction journals have different transactionId values; manual review is required and both were preserved.'
    }
    $phaseRank = @{ applying=0; 'committed-cleanup-pending'=1 }
    $currentRank = [int]$phaseRank[[string]$Current.phase]
    $previousRank = [int]$phaseRank[[string]$Previous.phase]
    if ($currentRank -lt $previousRank -or ($currentRank - $previousRank) -gt 1) {
        throw 'Current and previous transaction journal phases are not an allowed atomic progression; manual review is required and both were preserved.'
    }
    $currentOperations = @($Current.operations | ForEach-Object { $_ })
    $previousOperations = @($Previous.operations | ForEach-Object { $_ })
    $countDelta = $currentOperations.Count - $previousOperations.Count
    if ([Math]::Abs($countDelta) -gt 1) {
        throw 'Current and previous transaction journal operation counts are not adjacent states; manual review is required and both were preserved.'
    }
    if ($currentRank -ne $previousRank) {
        if ($countDelta -ne 0) {
            throw 'A journal phase transition must preserve the operation list; manual review is required and both were preserved.'
        }
        for ($index = 0; $index -lt $currentOperations.Count; $index++) {
            if ((Get-JournalOperationJson $currentOperations[$index]) -cne (Get-JournalOperationJson $previousOperations[$index])) {
                throw 'A journal phase transition changed operation state; manual review is required and both were preserved.'
            }
        }
        return
    }
    if ($countDelta -eq 0) {
        $changed = 0
        for ($index = 0; $index -lt $currentOperations.Count; $index++) {
            if ([string]$currentOperations[$index].id -cne [string]$previousOperations[$index].id) {
                throw 'Current and previous transaction journal operation order changed; manual review is required and both were preserved.'
            }
            if ((Get-JournalOperationJson $currentOperations[$index]) -cne (Get-JournalOperationJson $previousOperations[$index])) {
                $changed++
            }
        }
        if ($changed -gt 1) {
            throw 'More than one operation changed between atomic journal states; manual review is required and both were preserved.'
        }
        return
    }
    if ($countDelta -eq 1) {
        for ($index = 0; $index -lt $previousOperations.Count; $index++) {
            if ((Get-JournalOperationJson $currentOperations[$index]) -cne (Get-JournalOperationJson $previousOperations[$index])) {
                throw 'The current journal is not a one-operation append of the previous state; manual review is required and both were preserved.'
            }
        }
        return
    }
    $removedIndex = -1
    $currentIndex = 0
    for ($previousIndex = 0; $previousIndex -lt $previousOperations.Count; $previousIndex++) {
        if ($currentIndex -lt $currentOperations.Count -and
            (Get-JournalOperationJson $previousOperations[$previousIndex]) -ceq (Get-JournalOperationJson $currentOperations[$currentIndex])) {
            $currentIndex++
        } elseif ($removedIndex -lt 0) {
            $removedIndex = $previousIndex
        } else {
            throw 'The current journal is not a one-operation removal from the previous state; manual review is required and both were preserved.'
        }
    }
    if ($removedIndex -lt 0 -or $currentIndex -ne $currentOperations.Count) {
        throw 'The current and previous journal operation states are ambiguous; manual review is required and both were preserved.'
    }
}

function Remove-VerifiedTransactionFile {
    param(
        [string]$Path,
        [string]$Sha256,
        [long]$Length,
        [string]$Identity,
        [string[]]$AllowedRoots,
        [string]$Context
    )
    if (-not (Test-Path -LiteralPath $Path)) { return }
    Assert-InstallerFileState -Path $Path -Sha256 $Sha256 -Length $Length -Identity $Identity `
        -AllowedRoots $AllowedRoots -Context $Context
    Remove-SafeFile -Path $Path -AllowedRoots $AllowedRoots
}

function Set-OperationBackupState {
    param([object]$Operation, [object]$State)
    $Operation.backupSha256 = [string]$State.sha256
    $Operation.backupLength = [long]$State.length
    $Operation.backupIdentity = [string]$State.identity
}

function Set-VerifiedRecoveredBackupState {
    param(
        [object]$Journal,
        [object]$Operation,
        [string]$JournalPath,
        [string[]]$AllowedRoots
    )
    $state = Get-InstallerFileState -Path ([string]$Operation.backup) -AllowedRoots $AllowedRoots
    if ($state.sha256 -ne [string]$Operation.oldSha256 -or
        [long]$state.length -ne [long]$Operation.oldLength -or
        $state.identity -ne [string]$Operation.oldIdentity) {
        throw "Recovered transaction backup does not match persisted old state; manual review is required. Target, backup, and journal were preserved: $($Operation.backup)"
    }
    Set-OperationBackupState -Operation $Operation -State $state
    Write-InstallerJournal -Journal $Journal -JournalPath $JournalPath -AllowedRoots $AllowedRoots
}

function Remove-JournalOperation {
    param([object]$Journal, [object]$Operation, [string]$JournalPath, [string[]]$AllowedRoots)
    [void]$Journal.operations.Remove($Operation)
    Write-InstallerJournal -Journal $Journal -JournalPath $JournalPath -AllowedRoots $AllowedRoots
}

function Invoke-InstallerOperationRollback {
    param([object]$Journal, [object]$Operation, [string]$JournalPath, [string[]]$AllowedRoots)

    $target = [string]$Operation.target
    $backup = [string]$Operation.backup
    $temporary = [string]$Operation.temporary
    $kind = [string]$Operation.kind
    if ($kind -in @('Replace','ManifestReplace')) {
        if (Test-Path -LiteralPath $backup -PathType Leaf) {
            if ([string]::IsNullOrWhiteSpace([string]$Operation.backupIdentity)) {
                Set-VerifiedRecoveredBackupState -Journal $Journal -Operation $Operation `
                    -JournalPath $JournalPath -AllowedRoots $AllowedRoots
            }
            Assert-InstallerFileState -Path $target -Sha256 $Operation.newSha256 -Length $Operation.newLength `
                -Identity $Operation.tempIdentity -AllowedRoots $AllowedRoots -Context 'Rollback replacement target'
            Assert-InstallerFileState -Path $backup -Sha256 $Operation.backupSha256 -Length $Operation.backupLength `
                -Identity $Operation.backupIdentity -AllowedRoots $AllowedRoots -Context 'Rollback replacement backup'
            if ([string]::IsNullOrWhiteSpace([string]$Operation.rollback)) {
                $Operation.rollback = New-InstallerSiblingPath -Target $target -Kind 'rollback' `
                    -OperationId $Operation.id -AllowedRoots $AllowedRoots
                Write-InstallerJournal -Journal $Journal -JournalPath $JournalPath -AllowedRoots $AllowedRoots
            }
            Assert-SafePathForRoots -Path $target -AllowedRoots $AllowedRoots | Out-Null
            Assert-SafePathForRoots -Path $backup -AllowedRoots $AllowedRoots | Out-Null
            Assert-SafePathForRoots -Path $Operation.rollback -AllowedRoots $AllowedRoots | Out-Null
            [IO.File]::Replace($backup, $target, [string]$Operation.rollback, $true)
        }
        $restoreSha = if (-not [string]::IsNullOrWhiteSpace([string]$Operation.backupSha256)) { $Operation.backupSha256 } else { $Operation.oldSha256 }
        $restoreLength = if ([long]$Operation.backupLength -ge 0) { [long]$Operation.backupLength } else { [long]$Operation.oldLength }
        $restoreIdentity = if (-not [string]::IsNullOrWhiteSpace([string]$Operation.backupIdentity)) { $Operation.backupIdentity } else { $Operation.oldIdentity }
        if (Test-Path -LiteralPath $target -PathType Leaf) {
            Assert-InstallerFileState -Path $target -Sha256 $restoreSha -Length $restoreLength -Identity $restoreIdentity `
                -AllowedRoots $AllowedRoots -Context 'Restored replacement target'
        } else {
            throw "Rollback replacement target is missing: $target"
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$Operation.rollback) -and (Test-Path -LiteralPath $Operation.rollback)) {
            Remove-VerifiedTransactionFile -Path $Operation.rollback -Sha256 $Operation.newSha256 -Length $Operation.newLength `
                -Identity $Operation.tempIdentity -AllowedRoots $AllowedRoots -Context 'Rollback replacement discard'
        }
        if (Test-Path -LiteralPath $temporary) {
            Remove-VerifiedTransactionFile -Path $temporary -Sha256 $Operation.newSha256 -Length $Operation.newLength `
                -Identity $Operation.tempIdentity -AllowedRoots $AllowedRoots -Context 'Unused replacement temporary'
        }
    } elseif ($kind -in @('New','ManifestNew')) {
        if (Test-Path -LiteralPath $target -PathType Leaf) {
            if (Test-InstallerFileState -Path $target -Sha256 $Operation.newSha256 -Length $Operation.newLength `
                -Identity $Operation.tempIdentity -AllowedRoots $AllowedRoots) {
                Remove-SafeFile -Path $target -AllowedRoots $AllowedRoots
            } elseif ($Operation.applied) {
                throw "Rollback refuses to delete a New target whose identity changed: $target"
            }
        }
        if (Test-Path -LiteralPath $temporary) {
            Remove-VerifiedTransactionFile -Path $temporary -Sha256 $Operation.newSha256 -Length $Operation.newLength `
                -Identity $Operation.tempIdentity -AllowedRoots $AllowedRoots -Context 'Unused New temporary'
        }
    } elseif ($kind -eq 'Stale') {
        if (Test-Path -LiteralPath $backup -PathType Leaf) {
            if (Test-Path -LiteralPath $target) { throw "Rollback refuses to overwrite a recreated stale path: $target" }
            if ([string]::IsNullOrWhiteSpace([string]$Operation.backupIdentity)) {
                Set-VerifiedRecoveredBackupState -Journal $Journal -Operation $Operation `
                    -JournalPath $JournalPath -AllowedRoots $AllowedRoots
            }
            Assert-InstallerFileState -Path $backup -Sha256 $Operation.backupSha256 -Length $Operation.backupLength `
                -Identity $Operation.backupIdentity -AllowedRoots $AllowedRoots -Context 'Stale rollback backup'
            [IO.File]::Move($backup, $target)
        }
        $restoreSha = if (-not [string]::IsNullOrWhiteSpace([string]$Operation.backupSha256)) { $Operation.backupSha256 } else { $Operation.oldSha256 }
        $restoreLength = if ([long]$Operation.backupLength -ge 0) { [long]$Operation.backupLength } else { [long]$Operation.oldLength }
        $restoreIdentity = if (-not [string]::IsNullOrWhiteSpace([string]$Operation.backupIdentity)) { $Operation.backupIdentity } else { $Operation.oldIdentity }
        Assert-InstallerFileState -Path $target -Sha256 $restoreSha -Length $restoreLength -Identity $restoreIdentity `
            -AllowedRoots $AllowedRoots -Context 'Restored stale target'
    }
    Remove-JournalOperation -Journal $Journal -Operation $Operation -JournalPath $JournalPath -AllowedRoots $AllowedRoots
}

function Invoke-InstallerRollback {
    param([object]$Journal, [string]$JournalPath, [string[]]$AllowedRoots)
    $reverse = @($Journal.operations | ForEach-Object { $_ })
    [array]::Reverse($reverse)
    foreach ($operation in $reverse) {
        Invoke-InstallerOperationRollback -Journal $Journal -Operation $operation -JournalPath $JournalPath -AllowedRoots $AllowedRoots
    }
    $stageBase = Get-CanonicalInstallPath ([IO.Path]::GetTempPath())
    Assert-ExactInstallerStageRoot -StageRoot ([string]$Journal.stageRoot) `
        -TransactionId ([string]$Journal.transactionId) | Out-Null
    if (Test-Path -LiteralPath $Journal.stageRoot -PathType Container) {
        Remove-SafeTree -Root $Journal.stageRoot -Boundary $stageBase
    }
    Remove-SafeFile -Path $JournalPath -AllowedRoots $AllowedRoots
}

function Invoke-InstallerCommittedCleanup {
    param([object]$Journal, [string]$JournalPath, [string[]]$AllowedRoots)
    foreach ($operation in @($Journal.operations | ForEach-Object { $_ })) {
        if (-not [string]::IsNullOrWhiteSpace([string]$operation.backup) -and (Test-Path -LiteralPath $operation.backup)) {
            $backupSha = if (-not [string]::IsNullOrWhiteSpace([string]$operation.backupSha256)) { $operation.backupSha256 } else { $operation.oldSha256 }
            $backupLength = if ([long]$operation.backupLength -ge 0) { [long]$operation.backupLength } else { [long]$operation.oldLength }
            $backupIdentity = if (-not [string]::IsNullOrWhiteSpace([string]$operation.backupIdentity)) { $operation.backupIdentity } else { $operation.oldIdentity }
            Remove-VerifiedTransactionFile -Path $operation.backup -Sha256 $backupSha -Length $backupLength `
                -Identity $backupIdentity -AllowedRoots $AllowedRoots -Context 'Committed backup cleanup'
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$operation.temporary) -and (Test-Path -LiteralPath $operation.temporary)) {
            Remove-VerifiedTransactionFile -Path $operation.temporary -Sha256 $operation.newSha256 -Length $operation.newLength `
                -Identity $operation.tempIdentity -AllowedRoots $AllowedRoots -Context 'Committed temporary cleanup'
        }
        if (-not [string]::IsNullOrWhiteSpace([string]$operation.rollback) -and (Test-Path -LiteralPath $operation.rollback)) {
            Remove-VerifiedTransactionFile -Path $operation.rollback -Sha256 $operation.newSha256 -Length $operation.newLength `
                -Identity $operation.tempIdentity -AllowedRoots $AllowedRoots -Context 'Committed rollback-file cleanup'
        }
        Remove-JournalOperation -Journal $Journal -Operation $operation -JournalPath $JournalPath -AllowedRoots $AllowedRoots
    }
    $stageBase = Get-CanonicalInstallPath ([IO.Path]::GetTempPath())
    Assert-ExactInstallerStageRoot -StageRoot ([string]$Journal.stageRoot) `
        -TransactionId ([string]$Journal.transactionId) | Out-Null
    if (Test-Path -LiteralPath $Journal.stageRoot -PathType Container) {
        Remove-SafeTree -Root $Journal.stageRoot -Boundary $stageBase
    }
    Remove-SafeFile -Path $JournalPath -AllowedRoots $AllowedRoots
}

function Recover-InstallerTransaction {
    param(
        [string]$JournalPath,
        [string]$SourceRoot,
        [string]$InstallRoot,
        [string]$AddInRoot,
        [string]$Version
    )
    $allowedRoots = @($InstallRoot, $AddInRoot)
    $previousWrite = Join-Path $InstallRoot '.arcgis-pro-agent-journal-write-previous.bak'
    $currentRead = Read-StableStrictInstallerJournal -JournalPath $JournalPath -SourceRoot $SourceRoot `
        -InstallRoot $InstallRoot -AddInRoot $AddInRoot -Version $Version
    $previousRead = Read-StableStrictInstallerJournal -JournalPath $previousWrite -SourceRoot $SourceRoot `
        -InstallRoot $InstallRoot -AddInRoot $AddInRoot -Version $Version
    if ($null -eq $currentRead -and $null -ne $previousRead) {
        Assert-StableJournalFileState -JournalPath $previousWrite -ExpectedState $previousRead.State -AllowedRoots $allowedRoots
        if (Test-Path -LiteralPath $JournalPath) {
            throw 'The current transaction journal appeared during recovery; manual review is required and both files were preserved.'
        }
        Assert-SafePathForRoots -Path $previousWrite -AllowedRoots $allowedRoots | Out-Null
        Assert-SafePathForRoots -Path $JournalPath -AllowedRoots $allowedRoots | Out-Null
        [IO.File]::Move($previousWrite, $JournalPath)
        $currentRead = Read-StableStrictInstallerJournal -JournalPath $JournalPath -SourceRoot $SourceRoot `
            -InstallRoot $InstallRoot -AddInRoot $AddInRoot -Version $Version
        if ($currentRead.State.sha256 -ne $previousRead.State.sha256 -or
            [long]$currentRead.State.length -ne [long]$previousRead.State.length -or
            $currentRead.State.identity -ne $previousRead.State.identity) {
            throw 'The previous transaction journal changed while being promoted; manual review is required and the promoted file was preserved.'
        }
    } elseif ($null -ne $currentRead -and $null -ne $previousRead) {
        Assert-CompatibleAtomicJournalPair -Current $currentRead.Journal -Previous $previousRead.Journal
        Assert-StableJournalFileState -JournalPath $JournalPath -ExpectedState $currentRead.State -AllowedRoots $allowedRoots
        Assert-StableJournalFileState -JournalPath $previousWrite -ExpectedState $previousRead.State -AllowedRoots $allowedRoots
        Remove-SafeFile -Path $previousWrite -AllowedRoots $allowedRoots
    }
    if ($null -eq $currentRead) {
        $legacy = @(Get-ChildItem -LiteralPath $InstallRoot -Filter '.arcgis-pro-agent-install-journal-*.json' -File -Force -ErrorAction SilentlyContinue)
        if ($legacy.Count -gt 0) { throw 'A legacy transaction journal requires manual preservation and review.' }
        return
    }
    $journal = $currentRead.Journal
    if ([string]$journal.phase -eq 'applying') {
        Invoke-InstallerRollback -Journal $journal -JournalPath $JournalPath -AllowedRoots $allowedRoots
    } else {
        Invoke-InstallerCommittedCleanup -Journal $journal -JournalPath $JournalPath -AllowedRoots $allowedRoots
    }
}

function Invoke-ArcGISProAgentManifestInstall {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$SourceRoot,
        [Parameter(Mandatory = $true)][string]$InstallRoot,
        [Parameter(Mandatory = $true)][string]$AddInRoot,
        [Parameter(Mandatory = $true)][string]$Version,
        [Parameter(Mandatory = $true)][object[]]$Files,
        [string]$PreviousAddInRootWithoutEntries,
        [ValidateSet('None','Copy','Stale','ManifestCommit','Cleanup')][string]$FailurePoint = 'None',
        [scriptblock]$OperationHook
    )
    if ($Version -notmatch '^[0-9A-Za-z][0-9A-Za-z.-]*$' -or $Version.Contains('..')) {
        throw "Invalid product version segment: $Version"
    }
    $topology = Assert-SafeInstallTopology -SourceRoot $SourceRoot -InstallRoot $InstallRoot -AddInRoot $AddInRoot
    $SourceRoot = $topology.SourceRoot
    $InstallRoot = $topology.InstallRoot
    $AddInRoot = $topology.AddInRoot
    if (-not [string]::IsNullOrWhiteSpace($PreviousAddInRootWithoutEntries)) {
        $previousTopology = Assert-SafeInstallTopology -SourceRoot $SourceRoot -InstallRoot $InstallRoot `
            -AddInRoot $PreviousAddInRootWithoutEntries
        $PreviousAddInRootWithoutEntries = $previousTopology.AddInRoot
        if (-not $AddInRoot.Equals((Get-ArcGISProAgentDefaultAddInRoot), [StringComparison]::OrdinalIgnoreCase) -or
            -not $PreviousAddInRootWithoutEntries.Equals(
                (Get-ArcGISProAgentLegacyDefaultAddInRoot), [StringComparison]::OrdinalIgnoreCase)) {
            throw 'The previous Add-In root exception is limited to the exact legacy and official default roots.'
        }
    }
    Initialize-AuthorizedRoot -RootPath $InstallRoot -RootName 'InstallRoot' -OperationHook $OperationHook | Out-Null
    Initialize-AuthorizedRoot -RootPath $AddInRoot -RootName 'AddInRoot' -OperationHook $OperationHook | Out-Null
    $allowedRoots = @($InstallRoot, $AddInRoot)
    $lockPath = Get-CanonicalInstallPath (Join-Path $InstallRoot '.arcgis-pro-agent-install.lock')
    $journalPath = Get-CanonicalInstallPath (Join-Path $InstallRoot '.arcgis-pro-agent-install-journal.json')
    $manifestPath = Get-CanonicalInstallPath (Join-Path $InstallRoot 'install-manifest.json')
    foreach ($path in @($lockPath, $journalPath, $manifestPath)) {
        Assert-SafePathForRoots -Path $path -AllowedRoots $allowedRoots | Out-Null
    }
    $installerLock = $null
    $journal = $null
    try {
        try {
            $installerLock = New-Object IO.FileStream(
                $lockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        } catch {
            throw "Another installer is running or the exclusive installer lock cannot be acquired: $lockPath"
        }
        Assert-SafePathForRoots -Path $lockPath -AllowedRoots $allowedRoots | Out-Null
        if ($null -ne $OperationHook) { & $OperationHook 'LockAcquired' ([PSCustomObject]@{ LockPath=$lockPath }) }

        Recover-InstallerTransaction -JournalPath $journalPath -SourceRoot $SourceRoot -InstallRoot $InstallRoot `
            -AddInRoot $AddInRoot -Version $Version

        $previous = Read-StrictInstallManifest -ManifestPath $manifestPath -Version $Version `
            -InstallRoot $InstallRoot -AddInRoot $AddInRoot `
            -PreviousAddInRootWithoutEntries $PreviousAddInRootWithoutEntries
        $previousManifestState = $null
        $previousByPath = @{}
        if ($null -ne $previous) {
            $previousManifestState = Get-InstallerFileState -Path $manifestPath -AllowedRoots $allowedRoots
            foreach ($entry in @($previous.Entries)) {
                Assert-ExistingOwnedFileMatches -Entry $entry -AllowedRoots $allowedRoots
                $state = Get-InstallerFileState -Path $entry.path -AllowedRoots $allowedRoots
                $previousByPath[$entry.path.ToLowerInvariant()] = [PSCustomObject]@{ Entry=$entry; State=$state }
            }
        }

        $transactionId = [Guid]::NewGuid().ToString('N')
        $stageBase = Get-CanonicalInstallPath ([IO.Path]::GetTempPath())
        Assert-NoGisDataAncestor $stageBase
        Assert-NoReparsePointInPath $stageBase
        $stageRoot = Get-CanonicalInstallPath (Join-Path $stageBase ("ArcGISProAgent-install-$transactionId"))
        $operations = New-Object System.Collections.Generic.List[object]
        $journal = [PSCustomObject][ordered]@{
            schemaVersion = 2
            owner = $script:ManifestOwner
            version = $Version
            transactionId = $transactionId
            phase = 'applying'
            sourceRoot = $SourceRoot
            installRoot = $InstallRoot
            addInRoot = $AddInRoot
            manifestPath = $manifestPath
            stageRoot = $stageRoot
            operations = $operations
        }
        Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
        Ensure-SafeDirectory -Path $stageRoot -AllowedRoots @($stageRoot) | Out-Null

        $records = New-Object System.Collections.Generic.List[object]
        $seenNew = @{}
        $index = 0
        foreach ($file in $Files) {
            foreach ($required in @('SourcePath','DestinationPath','Component')) {
                if ($file.PSObject.Properties.Name -notcontains $required) { throw "Install input is missing property '$required'." }
            }
            $source = Assert-SafeSourceFile ([string]$file.SourcePath)
            $component = ([string]$file.Component).ToLowerInvariant()
            $destination = Assert-AllowedComponentPath -Path (Get-CanonicalInstallPath ([string]$file.DestinationPath)) `
                -Component $component -Version $Version -InstallRoot $InstallRoot -AddInRoot $AddInRoot
            Assert-SafePathForRoots -Path $destination -AllowedRoots $allowedRoots | Out-Null
            $key = $destination.ToLowerInvariant()
            if ($seenNew.ContainsKey($key)) { throw "Duplicate install destination: $destination" }
            $seenNew[$key] = $true
            if ((Test-Path -LiteralPath $destination) -and -not $previousByPath.ContainsKey($key)) {
                throw "Refusing to overwrite an unowned target: $destination"
            }
            $stageComponent = Join-Path $stageRoot $component
            Ensure-SafeDirectory -Path $stageComponent -AllowedRoots @($stageRoot) | Out-Null
            $stagePath = Get-CanonicalInstallPath (Join-Path $stageComponent ("$index-" + [IO.Path]::GetFileName($source)))
            Assert-SafeSourceFile $source | Out-Null
            [IO.File]::Copy($source, $stagePath, $false)
            $stageState = Get-InstallerFileState -Path $stagePath -AllowedRoots @($stageRoot)
            $records.Add([PSCustomObject]@{
                owner=$script:ManifestOwner; version=$Version; component=$component; path=$destination
                sha256=$stageState.sha256; length=$stageState.length; stagePath=$stagePath
            })
            $index++
        }

        $newByPath = @{}
        foreach ($record in $records) { $newByPath[$record.path.ToLowerInvariant()] = $record }
        $staleEntries = @($previousByPath.Values | Where-Object { -not $newByPath.ContainsKey($_.Entry.path.ToLowerInvariant()) })
        Ensure-SafeDirectory -Path $AddInRoot -AllowedRoots $allowedRoots | Out-Null

        $copyCompleted = 0
        $replaceCompleted = 0
        foreach ($record in $records) {
            $target = [string]$record.path
            Ensure-SafeDirectory -Path (Split-Path -Parent $target) -AllowedRoots $allowedRoots | Out-Null
            Assert-SafePathForRoots -Path $target -AllowedRoots $allowedRoots | Out-Null
            $operationId = [Guid]::NewGuid().ToString('N')
            $temporary = New-InstallerSiblingPath -Target $target -Kind 'new' -OperationId $operationId -AllowedRoots $allowedRoots
            [IO.File]::Copy([string]$record.stagePath, $temporary, $false)
            $temporaryState = Get-InstallerFileState -Path $temporary -AllowedRoots $allowedRoots
            if ($temporaryState.sha256 -ne $record.sha256 -or [long]$temporaryState.length -ne [long]$record.length) {
                throw "Transaction copy hash or length mismatch: $target"
            }
            $key = $target.ToLowerInvariant()
            if (Test-Path -LiteralPath $target -PathType Leaf) {
                if (-not $previousByPath.ContainsKey($key)) { throw "Refusing to overwrite an unowned target: $target" }
                $oldState = $previousByPath[$key].State
                Assert-InstallerFileState -Path $target -Sha256 $oldState.sha256 -Length $oldState.length `
                    -Identity $oldState.identity -AllowedRoots $allowedRoots -Context 'Owned target before Replace'
                $backup = New-InstallerSiblingPath -Target $target -Kind 'backup' -OperationId $operationId -AllowedRoots $allowedRoots
                $operation = New-InstallerJournalOperation -Kind 'Replace' -Component $record.component -Target $target `
                    -Backup $backup -Temporary $temporary -OldState $oldState -NewState $temporaryState
                $operation.id = $operationId
                $journal.operations.Add($operation)
                Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
                Assert-InstallerFileState -Path $target -Sha256 $oldState.sha256 -Length $oldState.length `
                    -Identity $oldState.identity -AllowedRoots $allowedRoots -Context 'Owned target immediately before Replace'
                if ($null -ne $OperationHook) { & $OperationHook 'BeforeReplace' ([PSCustomObject]@{ Target=$target; Backup=$backup; Temporary=$temporary }) }
                foreach ($path in @($target,$backup,$temporary)) { Assert-SafePathForRoots -Path $path -AllowedRoots $allowedRoots | Out-Null }
                [IO.File]::Replace($temporary, $target, $backup, $true)
                if ($replaceCompleted -eq 0 -and $null -ne $OperationHook) {
                    & $OperationHook 'AfterFirstReplace' ([PSCustomObject]@{ Target=$target; Backup=$backup })
                }
                $replaceCompleted++
                $backupState = Get-InstallerFileState -Path $backup -AllowedRoots $allowedRoots
                Set-OperationBackupState -Operation $operation -State $backupState
                Assert-InstallerFileState -Path $target -Sha256 $operation.newSha256 -Length $operation.newLength `
                    -Identity $operation.tempIdentity -AllowedRoots $allowedRoots -Context 'Installed replacement target'
                if ($backupState.sha256 -ne $oldState.sha256 -or [long]$backupState.length -ne [long]$oldState.length -or
                    $backupState.identity -ne $oldState.identity) {
                    Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
                    throw "Replacement backup hash, length, or identity changed concurrently: $target"
                }
                $operation.applied = $true
                Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
            } elseif (Test-Path -LiteralPath $target) {
                throw "Install target is not a file: $target"
            } else {
                $operation = New-InstallerJournalOperation -Kind 'New' -Component $record.component -Target $target `
                    -Backup '' -Temporary $temporary -OldState $null -NewState $temporaryState
                $operation.id = $operationId
                $journal.operations.Add($operation)
                Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
                if ($null -ne $OperationHook) { & $OperationHook 'BeforeNewMove' ([PSCustomObject]@{ Target=$target; Temporary=$temporary }) }
                Assert-SafePathForRoots -Path $target -AllowedRoots $allowedRoots | Out-Null
                Assert-SafePathForRoots -Path $temporary -AllowedRoots $allowedRoots | Out-Null
                if (Test-Path -LiteralPath $target) { throw "An unowned target appeared before the New move: $target" }
                [IO.File]::Move($temporary, $target)
                Assert-InstallerFileState -Path $target -Sha256 $operation.newSha256 -Length $operation.newLength `
                    -Identity $operation.tempIdentity -AllowedRoots $allowedRoots -Context 'Installed New target'
                $operation.applied = $true
                Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
            }
            $copyCompleted++
            if ($FailurePoint -eq 'Copy' -and $copyCompleted -eq 1) { throw 'Injected failure at Copy.' }
        }

        $staleCompleted = 0
        foreach ($prior in $staleEntries) {
            $entry = $prior.Entry
            $oldState = $prior.State
            Assert-InstallerFileState -Path $entry.path -Sha256 $oldState.sha256 -Length $oldState.length `
                -Identity $oldState.identity -AllowedRoots $allowedRoots -Context 'Owned stale target before move'
            $operationId = [Guid]::NewGuid().ToString('N')
            $backup = New-InstallerSiblingPath -Target $entry.path -Kind 'backup' -OperationId $operationId -AllowedRoots $allowedRoots
            $operation = New-InstallerJournalOperation -Kind 'Stale' -Component $entry.component -Target $entry.path `
                -Backup $backup -Temporary '' -OldState $oldState -NewState $null
            $operation.id = $operationId
            $journal.operations.Add($operation)
            Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
            Assert-InstallerFileState -Path $entry.path -Sha256 $oldState.sha256 -Length $oldState.length `
                -Identity $oldState.identity -AllowedRoots $allowedRoots -Context 'Owned stale target immediately before move'
            foreach ($path in @($entry.path,$backup)) { Assert-SafePathForRoots -Path $path -AllowedRoots $allowedRoots | Out-Null }
            [IO.File]::Move([string]$entry.path, $backup)
            if ($null -ne $OperationHook) {
                & $OperationHook 'AfterStaleMoveBeforeBackupState' ([PSCustomObject]@{ Target=$entry.path; Backup=$backup })
            }
            $backupState = Get-InstallerFileState -Path $backup -AllowedRoots $allowedRoots
            Set-OperationBackupState -Operation $operation -State $backupState
            if ($backupState.sha256 -ne $oldState.sha256 -or [long]$backupState.length -ne [long]$oldState.length -or
                $backupState.identity -ne $oldState.identity) {
                Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
                throw "Stale backup hash, length, or identity changed concurrently: $($entry.path)"
            }
            $operation.applied = $true
            Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
            $staleCompleted++
            if ($FailurePoint -eq 'Stale' -and $staleCompleted -eq 1) { throw 'Injected failure at Stale.' }
        }

        $manifestFiles = @($records | ForEach-Object {
            [ordered]@{ owner=$_.owner; version=$_.version; component=$_.component; path=$_.path; sha256=$_.sha256; length=$_.length }
        })
        $manifest = [ordered]@{
            schemaVersion=$script:ManifestSchemaVersion; owner=$script:ManifestOwner; version=$Version
            generatedAtUtc=[DateTime]::UtcNow.ToString('o'); manifestPath=$manifestPath
            installRoot=$InstallRoot; addInRoot=$AddInRoot; files=$manifestFiles
        }
        $manifestText = $manifest | ConvertTo-Json -Depth 7
        $manifestOperationId = [Guid]::NewGuid().ToString('N')
        $manifestTemporary = New-InstallerSiblingPath -Target $manifestPath -Kind 'new' `
            -OperationId $manifestOperationId -AllowedRoots $allowedRoots
        [IO.File]::WriteAllText($manifestTemporary, $manifestText, (New-Object Text.UTF8Encoding($false)))
        $manifestNewState = Get-InstallerFileState -Path $manifestTemporary -AllowedRoots $allowedRoots
        if (Test-Path -LiteralPath $manifestPath -PathType Leaf) {
            Assert-InstallerFileState -Path $manifestPath -Sha256 $previousManifestState.sha256 -Length $previousManifestState.length `
                -Identity $previousManifestState.identity -AllowedRoots $allowedRoots -Context 'Manifest immediately before Replace'
            $manifestBackup = New-InstallerSiblingPath -Target $manifestPath -Kind 'backup' `
                -OperationId $manifestOperationId -AllowedRoots $allowedRoots
            $manifestOperation = New-InstallerJournalOperation -Kind 'ManifestReplace' -Component 'manifest' `
                -Target $manifestPath -Backup $manifestBackup -Temporary $manifestTemporary `
                -OldState $previousManifestState -NewState $manifestNewState
            $manifestOperation.id = $manifestOperationId
            $journal.operations.Add($manifestOperation)
            Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
            Assert-InstallerFileState -Path $manifestPath -Sha256 $previousManifestState.sha256 -Length $previousManifestState.length `
                -Identity $previousManifestState.identity -AllowedRoots $allowedRoots -Context 'Manifest before atomic Replace'
            [IO.File]::Replace($manifestTemporary, $manifestPath, $manifestBackup, $true)
            if ($null -ne $OperationHook) {
                & $OperationHook 'AfterManifestReplaceBeforeBackupState' ([PSCustomObject]@{ Target=$manifestPath; Backup=$manifestBackup })
            }
            $manifestBackupState = Get-InstallerFileState -Path $manifestBackup -AllowedRoots $allowedRoots
            Set-OperationBackupState -Operation $manifestOperation -State $manifestBackupState
            Assert-InstallerFileState -Path $manifestPath -Sha256 $manifestNewState.sha256 -Length $manifestNewState.length `
                -Identity $manifestNewState.identity -AllowedRoots $allowedRoots -Context 'Committed manifest target'
            if ($manifestBackupState.sha256 -ne $previousManifestState.sha256 -or
                [long]$manifestBackupState.length -ne [long]$previousManifestState.length -or
                $manifestBackupState.identity -ne $previousManifestState.identity) {
                Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
                throw 'Manifest replacement backup changed concurrently.'
            }
            $manifestOperation.applied = $true
            Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
        } elseif (Test-Path -LiteralPath $manifestPath) {
            throw "Manifest target is not a file: $manifestPath"
        } else {
            $manifestOperation = New-InstallerJournalOperation -Kind 'ManifestNew' -Component 'manifest' `
                -Target $manifestPath -Backup '' -Temporary $manifestTemporary -OldState $null -NewState $manifestNewState
            $manifestOperation.id = $manifestOperationId
            $journal.operations.Add($manifestOperation)
            Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
            if (Test-Path -LiteralPath $manifestPath) { throw "An unowned manifest appeared before the New move: $manifestPath" }
            [IO.File]::Move($manifestTemporary, $manifestPath)
            Assert-InstallerFileState -Path $manifestPath -Sha256 $manifestNewState.sha256 -Length $manifestNewState.length `
                -Identity $manifestNewState.identity -AllowedRoots $allowedRoots -Context 'New manifest target'
            $manifestOperation.applied = $true
            Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
        }
        if ($FailurePoint -eq 'ManifestCommit') { throw 'Injected failure at ManifestCommit.' }

        $journal.phase = 'committed-cleanup-pending'
        Write-InstallerJournal -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
        if ($null -ne $OperationHook) {
            $firstBackupOperation = @($journal.operations | Where-Object {
                -not [string]::IsNullOrWhiteSpace([string]$_.backup) -and
                (Test-Path -LiteralPath ([string]$_.backup) -PathType Leaf)
            } | Select-Object -First 1)
            if ($firstBackupOperation.Count -eq 1) {
                & $OperationHook 'BeforeCommittedCleanup' ([PSCustomObject]@{
                    Backup = [string]$firstBackupOperation[0].backup
                    Target = [string]$firstBackupOperation[0].target
                })
            }
        }
        if ($FailurePoint -eq 'Cleanup') { throw 'Injected failure at committed cleanup.' }
        Invoke-InstallerCommittedCleanup -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
        $journal = $null
        [PSCustomObject]@{ ManifestPath=$manifestPath; FileCount=$records.Count }
    } catch {
        $original = $_
        if ($null -ne $journal -and [string]$journal.phase -eq 'applying' -and (Test-Path -LiteralPath $journalPath)) {
            try {
                Invoke-InstallerRollback -Journal $journal -JournalPath $journalPath -AllowedRoots $allowedRoots
                $journal = $null
            } catch {
                throw "$($original.Exception.Message) Rollback error: $($_.Exception.Message)"
            }
        }
        throw $original
    } finally {
        if ($null -ne $installerLock) { $installerLock.Dispose() }
    }
}

function Get-ArcGISProAgentMigrationSelectorRoot {
    param(
        [string]$InstallRoot,
        [string]$DefaultAddInRoot,
        [string]$LegacyAddInRoot
    )
    $install = Get-CanonicalInstallPath $InstallRoot
    $expectedRoots = @(
        Get-CanonicalInstallPath $DefaultAddInRoot
        Get-CanonicalInstallPath $LegacyAddInRoot
    )
    $readRoots = {
        param([string[]]$Paths, [string]$Context)
        $roots = New-Object System.Collections.Generic.List[string]
        foreach ($path in $Paths) {
            $canonicalPath = Get-CanonicalInstallPath $path
            Assert-SafePathForRoots -Path $canonicalPath -AllowedRoots @($install) | Out-Null
            if (-not (Test-Path -LiteralPath $canonicalPath)) { continue }
            if (-not (Test-Path -LiteralPath $canonicalPath -PathType Leaf)) {
                throw "$Context selector path is not a file: $canonicalPath"
            }
            $before = Get-InstallerFileState -Path $canonicalPath -AllowedRoots @($install)
            try { $raw = Get-Content -LiteralPath $canonicalPath -Raw | ConvertFrom-Json }
            catch { throw "$Context selector JSON is invalid and was preserved: $($_.Exception.Message)" }
            $declared = Assert-FullyQualifiedCanonicalPath `
                ([string](Get-RequiredProperty $raw 'addInRoot' "$Context root")) "$Context addInRoot"
            if (@($expectedRoots | Where-Object {
                    $_.Equals($declared, [StringComparison]::OrdinalIgnoreCase)
                }).Count -ne 1) {
                throw "$Context addInRoot is not an exact supported migration root."
            }
            $after = Get-InstallerFileState -Path $canonicalPath -AllowedRoots @($install)
            if ($before.sha256 -ne $after.sha256 -or [long]$before.length -ne [long]$after.length -or
                $before.identity -ne $after.identity) {
                throw "$Context selector changed during stable inspection."
            }
            $roots.Add($declared)
        }
        if ($roots.Count -eq 0) { return $null }
        $first = $roots[0]
        if (@($roots | Where-Object {
                -not $_.Equals($first, [StringComparison]::OrdinalIgnoreCase)
            }).Count -ne 0) {
            throw "$Context selectors disagree about addInRoot and require manual review."
        }
        $first
    }

    $journalRoot = & $readRoots @(
        (Join-Path $install '.arcgis-pro-agent-install-journal.json'),
        (Join-Path $install '.arcgis-pro-agent-journal-write-previous.bak')
    ) 'Transaction journal'
    if ($null -ne $journalRoot) {
        return [PSCustomObject]@{ Kind='Journal'; AddInRoot=$journalRoot }
    }
    $manifestRoot = & $readRoots @((Join-Path $install 'install-manifest.json')) 'Manifest'
    [PSCustomObject]@{ Kind='Manifest'; AddInRoot=$manifestRoot }
}

function Invoke-ArcGISProAgentManifestInstallWithDefaultAddInMigration {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$SourceRoot,
        [Parameter(Mandatory = $true)][string]$InstallRoot,
        [Parameter(Mandatory = $true)][string]$AddInRoot,
        [Parameter(Mandatory = $true)][string]$Version,
        [Parameter(Mandatory = $true)][object[]]$Files,
        [ValidateSet('None','Copy','Stale','ManifestCommit','Cleanup')][string]$LegacyFailurePoint = 'None',
        [ValidateSet('None','Copy','Stale','ManifestCommit','Cleanup')][string]$FailurePoint = 'None',
        [scriptblock]$OperationHook
    )
    $topology = Assert-SafeInstallTopology -SourceRoot $SourceRoot -InstallRoot $InstallRoot -AddInRoot $AddInRoot
    $SourceRoot = $topology.SourceRoot
    $InstallRoot = $topology.InstallRoot
    $AddInRoot = $topology.AddInRoot
    $defaultRoot = Get-ArcGISProAgentDefaultAddInRoot
    if (-not $AddInRoot.Equals($defaultRoot, [StringComparison]::OrdinalIgnoreCase)) {
        return Invoke-ArcGISProAgentManifestInstall -SourceRoot $SourceRoot -InstallRoot $InstallRoot `
            -AddInRoot $AddInRoot -Version $Version -Files $Files -FailurePoint $FailurePoint `
            -OperationHook $OperationHook
    }

    $legacyRoot = Get-ArcGISProAgentLegacyDefaultAddInRoot
    Assert-SafeInstallTopology -SourceRoot $SourceRoot -InstallRoot $InstallRoot -AddInRoot $legacyRoot | Out-Null
    $selector = Get-ArcGISProAgentMigrationSelectorRoot -InstallRoot $InstallRoot `
        -DefaultAddInRoot $defaultRoot -LegacyAddInRoot $legacyRoot
    $journalIsCurrent = $selector.Kind -eq 'Journal' -and $null -ne $selector.AddInRoot -and
        $selector.AddInRoot.Equals($defaultRoot, [StringComparison]::OrdinalIgnoreCase)
    if ($journalIsCurrent) {
        return Invoke-ArcGISProAgentManifestInstall -SourceRoot $SourceRoot -InstallRoot $InstallRoot `
            -AddInRoot $AddInRoot -PreviousAddInRootWithoutEntries $legacyRoot -Version $Version `
            -Files $Files -FailurePoint $FailurePoint -OperationHook $OperationHook
    }

    $requiresLegacyTransaction = $null -ne $selector.AddInRoot -and
        $selector.AddInRoot.Equals($legacyRoot, [StringComparison]::OrdinalIgnoreCase)
    if ($requiresLegacyTransaction) {
        $nonAddInFiles = New-Object System.Collections.Generic.List[object]
        foreach ($file in $Files) {
            if ($file.PSObject.Properties.Name -notcontains 'Component') {
                throw "Install input is missing property 'Component'."
            }
            if (-not ([string]$file.Component).Equals('addin', [StringComparison]::OrdinalIgnoreCase)) {
                $nonAddInFiles.Add($file)
            }
        }
        Invoke-ArcGISProAgentManifestInstall -SourceRoot $SourceRoot -InstallRoot $InstallRoot `
            -AddInRoot $legacyRoot -Version $Version `
            -Files @($nonAddInFiles | ForEach-Object { $_ }) -FailurePoint $LegacyFailurePoint `
            -OperationHook $OperationHook | Out-Null
    }

    Invoke-ArcGISProAgentManifestInstall -SourceRoot $SourceRoot -InstallRoot $InstallRoot `
        -AddInRoot $AddInRoot -PreviousAddInRootWithoutEntries $legacyRoot -Version $Version `
        -Files $Files -FailurePoint $FailurePoint -OperationHook $OperationHook
}

Export-ModuleMember -Function @(
    'Get-ArcGISProAgentDefaultAddInRoot',
    'Get-ArcGISProAgentLegacyDefaultAddInRoot',
    'Invoke-ArcGISProAgentManifestInstall',
    'Invoke-ArcGISProAgentManifestInstallWithDefaultAddInMigration',
    'Test-ArcGISProAgentInstallTopology'
)
