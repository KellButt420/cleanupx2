<#
.SYNOPSIS
    Builds a disposable per-user Windows Installer package for Cleanup tests.

.DESCRIPTION
    Authors a minimal MSI directly through msi.dll (no external toolset). The
    package installs per-user without elevation and writes one registry value
    under HKCU\Software\CleanupDisposableTestMsi, so installing and removing it
    touches nothing else. Windows Installer registers it in "Apps" like any
    other product.

    Install:   msiexec /i <path> /qn
    Remove:    msiexec /x {6E1C3A52-7B1D-4F0A-9C3E-2D5B8A41F7C9} /qn
#>
param(
    [Parameter(Mandatory)] [string] $OutputPath
)

$ErrorActionPreference = 'Stop'
$ProductCode = '{6E1C3A52-7B1D-4F0A-9C3E-2D5B8A41F7C9}'
$UpgradeCode = '{0B8F4E7D-3C2A-4D61-8E5F-7A9C1B2D3E4F}'
$ComponentId = '{A4D2C6E8-1F3B-4A5C-9D7E-8B6F0C2E4A1D}'

if (-not ('MsiAuthoring' -as [type])) { Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class MsiAuthoring
{
    [DllImport("msi.dll", CharSet = CharSet.Unicode)] static extern uint MsiOpenDatabaseW(string path, IntPtr persist, out IntPtr db);
    [DllImport("msi.dll", CharSet = CharSet.Unicode)] static extern uint MsiDatabaseOpenViewW(IntPtr db, string query, out IntPtr view);
    [DllImport("msi.dll")] static extern uint MsiViewExecute(IntPtr view, IntPtr record);
    [DllImport("msi.dll")] static extern uint MsiViewClose(IntPtr view);
    [DllImport("msi.dll")] static extern uint MsiCloseHandle(IntPtr handle);
    [DllImport("msi.dll")] static extern uint MsiDatabaseCommit(IntPtr db);
    [DllImport("msi.dll", CharSet = CharSet.Unicode)] static extern uint MsiGetSummaryInformationW(IntPtr db, string path, uint updateCount, out IntPtr summary);
    [DllImport("msi.dll", CharSet = CharSet.Unicode)] static extern uint MsiSummaryInfoSetPropertyW(IntPtr summary, uint property, uint dataType, int intValue, IntPtr fileTime, string text);
    [DllImport("msi.dll")] static extern uint MsiSummaryInfoPersist(IntPtr summary);

    const uint VT_I2 = 2, VT_I4 = 3, VT_LPSTR = 30;

    static void Check(uint rc, string what) { if (rc != 0) throw new InvalidOperationException(what + " failed: " + rc); }

    public static void Build(string path, string[] statements, string packageCode, string subject, string author)
    {
        IntPtr db;
        Check(MsiOpenDatabaseW(path, (IntPtr)3 /* MSIDBOPEN_CREATE */, out db), "MsiOpenDatabase");
        try
        {
            foreach (var sql in statements)
            {
                IntPtr view;
                Check(MsiDatabaseOpenViewW(db, sql, out view), "OpenView: " + sql);
                try { Check(MsiViewExecute(view, IntPtr.Zero), "Execute: " + sql); }
                finally { MsiViewClose(view); MsiCloseHandle(view); }
            }

            IntPtr summary;
            Check(MsiGetSummaryInformationW(db, null, 20, out summary), "MsiGetSummaryInformation");
            try
            {
                Check(MsiSummaryInfoSetPropertyW(summary, 1, VT_I2, 1252, IntPtr.Zero, null), "Codepage");
                Check(MsiSummaryInfoSetPropertyW(summary, 2, VT_LPSTR, 0, IntPtr.Zero, "Installation Database"), "Title");
                Check(MsiSummaryInfoSetPropertyW(summary, 3, VT_LPSTR, 0, IntPtr.Zero, subject), "Subject");
                Check(MsiSummaryInfoSetPropertyW(summary, 4, VT_LPSTR, 0, IntPtr.Zero, author), "Author");
                Check(MsiSummaryInfoSetPropertyW(summary, 7, VT_LPSTR, 0, IntPtr.Zero, "x64;1033"), "Template");
                Check(MsiSummaryInfoSetPropertyW(summary, 9, VT_LPSTR, 0, IntPtr.Zero, packageCode), "Revision");
                Check(MsiSummaryInfoSetPropertyW(summary, 14, VT_I4, 200, IntPtr.Zero, null), "PageCount");
                // 2 = compressed source, 8 = elevated privileges are not required.
                Check(MsiSummaryInfoSetPropertyW(summary, 15, VT_I4, 10, IntPtr.Zero, null), "WordCount");
                Check(MsiSummaryInfoPersist(summary), "MsiSummaryInfoPersist");
            }
            finally { MsiCloseHandle(summary); }

            Check(MsiDatabaseCommit(db), "MsiDatabaseCommit");
        }
        finally { MsiCloseHandle(db); }
    }
}
'@
}

$statements = @(
    'CREATE TABLE `Property` (`Property` CHAR(72) NOT NULL, `Value` LONGCHAR NOT NULL LOCALIZABLE PRIMARY KEY `Property`)'
    'CREATE TABLE `Directory` (`Directory` CHAR(72) NOT NULL, `Directory_Parent` CHAR(72), `DefaultDir` CHAR(255) NOT NULL LOCALIZABLE PRIMARY KEY `Directory`)'
    'CREATE TABLE `Component` (`Component` CHAR(72) NOT NULL, `ComponentId` CHAR(38), `Directory_` CHAR(72) NOT NULL, `Attributes` SHORT NOT NULL, `Condition` CHAR(255), `KeyPath` CHAR(72) PRIMARY KEY `Component`)'
    'CREATE TABLE `Feature` (`Feature` CHAR(38) NOT NULL, `Feature_Parent` CHAR(38), `Title` CHAR(64) LOCALIZABLE, `Description` CHAR(255) LOCALIZABLE, `Display` SHORT, `Level` SHORT NOT NULL, `Directory_` CHAR(72), `Attributes` SHORT NOT NULL PRIMARY KEY `Feature`)'
    'CREATE TABLE `FeatureComponents` (`Feature_` CHAR(38) NOT NULL, `Component_` CHAR(72) NOT NULL PRIMARY KEY `Feature_`, `Component_`)'
    'CREATE TABLE `Registry` (`Registry` CHAR(72) NOT NULL, `Root` SHORT NOT NULL, `Key` CHAR(255) NOT NULL LOCALIZABLE, `Name` CHAR(255) LOCALIZABLE, `Value` LONGCHAR LOCALIZABLE, `Component_` CHAR(72) NOT NULL PRIMARY KEY `Registry`)'
    'CREATE TABLE `InstallExecuteSequence` (`Action` CHAR(72) NOT NULL, `Condition` CHAR(255), `Sequence` SHORT PRIMARY KEY `Action`)'
    # RegisterProduct reads the source list from Media even without files.
    'CREATE TABLE `Media` (`DiskId` SHORT NOT NULL, `LastSequence` SHORT NOT NULL, `DiskPrompt` CHAR(64) LOCALIZABLE, `Cabinet` CHAR(255), `VolumeLabel` CHAR(32), `Source` CHAR(72) PRIMARY KEY `DiskId`)'
    "INSERT INTO ``Media`` (``DiskId``, ``LastSequence``, ``DiskPrompt``, ``Cabinet``, ``VolumeLabel``, ``Source``) VALUES (1, 0, '', '', '', '')"
    "INSERT INTO ``Property`` (``Property``, ``Value``) VALUES ('ProductCode', '$ProductCode')"
    "INSERT INTO ``Property`` (``Property``, ``Value``) VALUES ('UpgradeCode', '$UpgradeCode')"
    "INSERT INTO ``Property`` (``Property``, ``Value``) VALUES ('ProductName', 'Cleanup Disposable Test MSI')"
    "INSERT INTO ``Property`` (``Property``, ``Value``) VALUES ('ProductVersion', '1.0.0')"
    "INSERT INTO ``Property`` (``Property``, ``Value``) VALUES ('Manufacturer', 'Cleanup Test Fixtures')"
    "INSERT INTO ``Property`` (``Property``, ``Value``) VALUES ('ProductLanguage', '1033')"
    "INSERT INTO ``Property`` (``Property``, ``Value``) VALUES ('ALLUSERS', '2')"
    "INSERT INTO ``Property`` (``Property``, ``Value``) VALUES ('MSIINSTALLPERUSER', '1')"
    "INSERT INTO ``Property`` (``Property``, ``Value``) VALUES ('ARPNOMODIFY', '1')"
    "INSERT INTO ``Property`` (``Property``, ``Value``) VALUES ('ARPNOREPAIR', '1')"
    "INSERT INTO ``Directory`` (``Directory``, ``Directory_Parent``, ``DefaultDir``) VALUES ('TARGETDIR', '', 'SourceDir')"
    "INSERT INTO ``Component`` (``Component``, ``ComponentId``, ``Directory_``, ``Attributes``, ``Condition``, ``KeyPath``) VALUES ('Main', '$ComponentId', 'TARGETDIR', 4, '', 'Marker')"
    "INSERT INTO ``Registry`` (``Registry``, ``Root``, ``Key``, ``Name``, ``Value``, ``Component_``) VALUES ('Marker', 1, 'Software\CleanupDisposableTestMsi', 'Installed', '1', 'Main')"
    "INSERT INTO ``Feature`` (``Feature``, ``Feature_Parent``, ``Title``, ``Description``, ``Display``, ``Level``, ``Directory_``, ``Attributes``) VALUES ('Main', '', 'Main', 'Disposable test feature', 0, 1, '', 0)"
    "INSERT INTO ``FeatureComponents`` (``Feature_``, ``Component_``) VALUES ('Main', 'Main')"
)
$sequence = [ordered]@{
    CostInitialize = 800; FileCost = 900; CostFinalize = 1000; InstallValidate = 1400
    InstallInitialize = 1500; ProcessComponents = 1600; UnpublishFeatures = 1800
    RemoveRegistryValues = 2600; WriteRegistryValues = 5000; RegisterUser = 6000
    RegisterProduct = 6100; PublishFeatures = 6300; PublishProduct = 6400; InstallFinalize = 6600
}
foreach ($action in $sequence.GetEnumerator()) {
    $statements += "INSERT INTO ``InstallExecuteSequence`` (``Action``, ``Condition``, ``Sequence``) VALUES ('$($action.Key)', '', $($action.Value))"
}

$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)
New-Item -ItemType Directory -Force -Path (Split-Path $OutputPath) | Out-Null
if (Test-Path $OutputPath) { Remove-Item $OutputPath -Force }
[MsiAuthoring]::Build($OutputPath, $statements, ('{' + [guid]::NewGuid().ToString().ToUpperInvariant() + '}'), 'Cleanup Disposable Test MSI', 'Cleanup Test Fixtures')
Write-Host "Built $OutputPath"
$ProductCode
