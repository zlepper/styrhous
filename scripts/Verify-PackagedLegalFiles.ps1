param(
    [Parameter(Mandatory = $true)]
    [string]$PackageRoot
)

$ErrorActionPreference = 'Stop'
$RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Manifest = Join-Path $RepositoryRoot 'legal/resources.tsv'

foreach ($Line in Get-Content $Manifest) {
    if ([string]::IsNullOrWhiteSpace($Line) -or $Line.StartsWith('#')) {
        continue
    }

    $Fields = $Line -split "`t"
    if ($Fields.Count -ne 3) {
        throw "Invalid legal resource manifest line: $Line"
    }

    $Source = Join-Path $RepositoryRoot $Fields[0]
    $PackagedFilename = $Fields[1]
    $Matches = @(Get-ChildItem -Path $PackageRoot -File -Recurse -Filter $PackagedFilename)
    if ($Matches.Count -ne 1) {
        throw "Expected exactly one $PackagedFilename in $PackageRoot, found $($Matches.Count)"
    }

    $SourceHash = (Get-FileHash -Algorithm SHA256 $Source).Hash
    $PackagedHash = (Get-FileHash -Algorithm SHA256 $Matches[0].FullName).Hash
    if ($SourceHash -ne $PackagedHash) {
        throw "Packaged $PackagedFilename does not match $($Fields[0])"
    }
}

Write-Output 'Packaged legal resources are present'
