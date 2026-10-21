<# Reads identities from the actual signed packages. Generates an update feed; never publishes or installs it. #>
param(
    [Parameter(Mandatory)] [string] $Package,
    [Parameter(Mandatory)] [uri] $ReleaseBaseUrl,
    [Parameter(Mandatory)] [uri] $FeedUrl,
    [string[]] $DependencyPackages = @(),
    [string] $Output = (Join-Path $PSScriptRoot '..\artifacts\release\cleanup.appinstaller')
)
$ErrorActionPreference = 'Stop'
if ($ReleaseBaseUrl.Scheme -ne 'https' -or $FeedUrl.Scheme -ne 'https' -or $ReleaseBaseUrl.UserInfo -or $FeedUrl.UserInfo) { throw 'Release and feed URLs must be credential-free HTTPS URLs.' }
function Read-Identity([string]$Path) {
    if ((Get-AuthenticodeSignature -LiteralPath $Path).Status -ne 'Valid') { throw "Package must have a valid trusted signature: $Path" }
    $archive = [IO.Compression.ZipFile]::OpenRead((Resolve-Path -LiteralPath $Path).Path)
    try {
        $entry = $archive.GetEntry('AppxManifest.xml')
        if (-not $entry) { throw 'Package has no AppxManifest.xml.' }
        $reader = [IO.StreamReader]::new($entry.Open())
        try { [xml]$xml = $reader.ReadToEnd() } finally { $reader.Dispose() }
        return $xml.Package.Identity
    } finally { $archive.Dispose() }
}
$identity = Read-Identity $Package
if ($identity.Name -match '(?i)\.dev$' -or $identity.Publisher -eq 'CN=Cleanup Dev') { throw 'Development identities cannot be published to the production update feed.' }
if ($identity.ProcessorArchitecture -ne 'x64') { throw 'Only the verified x64 release is supported.' }
$document = [xml]::new()
$declaration = $document.CreateXmlDeclaration('1.0', 'utf-8', $null)
[void]$document.AppendChild($declaration)
$namespace = 'http://schemas.microsoft.com/appx/appinstaller/2021'
$root = $document.CreateElement('AppInstaller', $namespace)
$root.SetAttribute('Uri', $FeedUrl.AbsoluteUri)
$root.SetAttribute('Version', [string]$identity.Version)
[void]$document.AppendChild($root)
function Add-PackageElement($Parent, $Identity, [string]$Path) {
    $element = $document.CreateElement($(if ($Parent -eq $root) { 'MainPackage' } else { 'Package' }), $namespace)
    foreach ($attribute in @('Name','Publisher','Version','ProcessorArchitecture')) { $element.SetAttribute($attribute, [string]$Identity.$attribute) }
    $element.SetAttribute('Uri', $ReleaseBaseUrl.AbsoluteUri.TrimEnd('/') + '/' + [uri]::EscapeDataString([IO.Path]::GetFileName($Path)))
    [void]$Parent.AppendChild($element)
}
Add-PackageElement $root $identity $Package
if ($DependencyPackages.Count) {
    $dependencies = $document.CreateElement('Dependencies', $namespace)
    [void]$root.AppendChild($dependencies)
    foreach ($dependency in $DependencyPackages) { Add-PackageElement $dependencies (Read-Identity $dependency) $dependency }
}
$settings = $document.CreateElement('UpdateSettings', $namespace)
$onLaunch = $document.CreateElement('OnLaunch', $namespace)
$onLaunch.SetAttribute('HoursBetweenUpdateChecks', '0')
$onLaunch.SetAttribute('ShowPrompt', 'true')
$onLaunch.SetAttribute('UpdateBlocksActivation', 'false')
[void]$settings.AppendChild($onLaunch)
[void]$settings.AppendChild($document.CreateElement('AutomaticBackgroundTask', $namespace))
[void]$root.AppendChild($settings)
$outputPath = [IO.Path]::GetFullPath($Output)
New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($outputPath)) -Force | Out-Null
$document.Save($outputPath)
Write-Output $outputPath
