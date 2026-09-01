[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$solution = Join-Path $PSScriptRoot 'McpServer.sln'
if (-not (Test-Path -LiteralPath $solution -PathType Leaf)) {
    throw "Solution was not found: $solution"
}

$devenv = Get-Command 'devenv.exe' -ErrorAction SilentlyContinue
if ($null -ne $devenv) {
    & $devenv.Source $solution
} else {
    Start-Process -FilePath $solution
}
