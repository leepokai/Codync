param(
    [Parameter(Mandatory = $true)]
    [string] $Executable,
    [string] $ExtensionId = 'dgejialhpalhoigbkmgeabgpphlipiek'
)

$ErrorActionPreference = 'Stop'
if ($ExtensionId -notmatch '^[a-p]{32}$') {
    throw "Invalid Chrome extension ID: $ExtensionId"
}
$helper = (Resolve-Path -LiteralPath $Executable).Path
if (-not (Test-Path -LiteralPath $helper -PathType Leaf)) {
    throw "Native helper not found: $helper"
}

if ($ExtensionId -ne 'dgejialhpalhoigbkmgeabgpphlipiek') {
    throw "The packaged extension ID is fixed; rebuild the manifest instead of overriding it"
}
& $helper --install-native-messaging
if ($LASTEXITCODE -ne 0) {
    throw "The native-messaging helper could not install its manifest (exit $LASTEXITCODE)"
}

$variants = @(
    [pscustomobject]@{
        HostName = 'codync.browser'
        Manifest = Join-Path $env:LOCALAPPDATA 'Codync\BrowserExtension\codync.browser.json'
    },
    [pscustomobject]@{
        HostName = 'codync.browser.dev'
        Manifest = Join-Path $env:LOCALAPPDATA 'Codync Dev\BrowserExtension\codync.browser.dev.json'
    }
)
$installed = foreach ($variant in $variants) {
    if (-not (Test-Path -LiteralPath $variant.Manifest -PathType Leaf)) {
        continue
    }
    $candidateText = [System.IO.File]::ReadAllText($variant.Manifest, [System.Text.UTF8Encoding]::new($false))
    $candidate = $candidateText | ConvertFrom-Json
    if ($candidate.path -eq $helper) {
        [pscustomobject]@{
            HostName = $variant.HostName
            ManifestPath = $variant.Manifest
        }
    }
}
$installed = @($installed)
if ($installed.Count -ne 1) {
    throw "Expected exactly one native-messaging manifest for $helper, found $($installed.Count)"
}
$hostName = $installed[0].HostName
$manifestPath = $installed[0].ManifestPath
$manifestText = [System.IO.File]::ReadAllText($manifestPath, [System.Text.UTF8Encoding]::new($false))
$manifest = $manifestText | ConvertFrom-Json
if ($manifest.name -ne $hostName -or $manifest.path -ne $helper) {
    throw "The installed native-messaging manifest does not match the selected helper"
}

[pscustomobject]@{
    NativeHost = $hostName
    Executable = $helper
    Manifest = $manifestPath
    ExtensionId = $ExtensionId
    ChromeRegistry = "HKCU:\Software\Google\Chrome\NativeMessagingHosts\$hostName"
    EdgeRegistry = "HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\$hostName"
}
