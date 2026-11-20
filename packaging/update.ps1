# Portable package update service. The Rust UI owns the two choices.
[CmdletBinding()]
param([int]$ParentId, [string]$CurrentVersion)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$repository = 'sheetau/OkColorPicker'
# All package members are regular files in one directory. Commit the executable last.
$members = @('BUNDLE.json','DXC_SOURCES.json','SOURCE_INDEX.json','SOURCES.zip',
    'THIRD_PARTY_NOTICES.html','START_HERE.txt','OkColorPicker.lua','update.ps1','PACKAGE.json','OkColorPicker.exe')

function Get-ChildPath([string]$Root, [string]$Name) {
    if ([IO.Path]::GetFileName($Name) -cne $Name -or $Name -in '.', '..') { throw 'Expected a single file name.' }
    $path = [IO.Path]::GetFullPath([IO.Path]::Combine($Root,$Name))
    if ([IO.Path]::GetDirectoryName($path) -ine $Root) { throw 'Path is outside the package directory.' }
    if ((Test-Path -LiteralPath $path) -and ((Get-Item -Force -LiteralPath $path).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Unexpected reparse point: $Name"
    }
    $path
}

function Get-ReleaseAsset($Release, [version]$Installed) {
    if ($Release.draft -or $Release.prerelease -or $Release.tag_name -cnotmatch '^v?(\d+\.\d+\.\d+)$') { return }
    $version = $Matches[1]
    if ([version]$version -le $Installed) { return }
    $pattern = '^OkColorPicker-' + [regex]::Escape($version) + '-windows-x64-[a-f0-9]{12}\.zip$'
    $assets = @($Release.assets | Where-Object { $_.name -cmatch $pattern -and $_.state -eq 'uploaded' })
    if ($assets.Count -ne 1) { throw 'Expected one published Windows x64 package.' }
    $asset = $assets[0]
    $url = "https://github.com/$repository/releases/download/$($Release.tag_name)/$($asset.name)"
    if ($asset.browser_download_url -cne $url -or $asset.digest -cnotmatch '^sha256:[a-f0-9]{64}$' -or
        $asset.size -lt 1 -or $asset.size -gt 536870912) { throw 'Invalid release metadata.' }
    [pscustomobject]@{version=$version; name=$asset.name; url=$url; sha256=$asset.digest.Substring(7); size=$asset.size}
}

function Get-Sha256([string]$Path) {
    $algorithm = [Security.Cryptography.SHA256]::Create()
    $stream = [IO.File]::OpenRead($Path)
    try { [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-','') }
    finally { $stream.Dispose(); $algorithm.Dispose() }
}

function Assert-Archive($Asset, [string]$Archive) {
    if ((Get-Item -LiteralPath $Archive).Length -ne $Asset.size -or (Get-Sha256 $Archive) -ine $Asset.sha256) {
        throw 'Release ZIP checksum or size mismatch.'
    }
}

function Expand-Package($Asset,[string]$Archive,[string]$Stage) {
    Assert-Archive $Asset $Archive
    $version = [string]$Asset.version
    if ($version -cnotmatch '^\d+\.\d+\.\d+$' -or $Asset.name -cnotmatch ('^OkColorPicker-'+[regex]::Escape($version)+'-windows-x64-([a-f0-9]{12})\.zip$')) {
        throw 'Invalid release identity.'
    }
    $prefix = $Matches[1]
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        if ($zip.Entries.Count -ne $members.Count) { throw 'Unexpected ZIP contents.' }
        $total = 0L
        foreach ($name in $members) {
            $entry = $zip.GetEntry([IO.Path]::GetFileNameWithoutExtension($Asset.name)+'/'+$name)
            if ($null -eq $entry) { throw "Missing package member: $name" }
            $total += $entry.Length
            if ($total -gt 1073741824) { throw 'Package exceeds its expanded size limit.' }
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry,(Get-ChildPath $Stage $name))
        }
    } finally { $zip.Dispose() }
    $meta = Get-Content -LiteralPath (Get-ChildPath $Stage 'PACKAGE.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($meta.version -cne $version -or $meta.target -cne 'x86_64-pc-windows-msvc' -or
        $meta.project_commit -cnotmatch '^[a-f0-9]{40}$' -or !$meta.project_commit.StartsWith($prefix)) { throw 'Package identity mismatch.' }
    foreach ($name in $members) {
        if ($name -ne 'PACKAGE.json' -and (Get-Sha256 (Get-ChildPath $Stage $name)) -ine $meta.files.$name) {
            throw "Package checksum mismatch: $name"
        }
    }
}

function Install-Package([string]$Root,[string]$Stage) {
    $installed = [Collections.Generic.List[string]]::new()
    try {
        foreach ($name in $members) {
            $to = Get-ChildPath $Root $name
            $from = Get-ChildPath $Stage $name
            # Only update an existing installed package, never claim an unrelated file or directory.
            if (![IO.File]::Exists($to)) { throw "Installed package member missing: $name" }
            [IO.File]::Replace($from,$to,(Get-ChildPath $Stage ($name+'.previous')))
            $installed.Add($name)
        }
    } catch {
        $installed.Reverse()
        foreach ($name in $installed) {
            [IO.File]::Replace((Get-ChildPath $Stage ($name+'.previous')),(Get-ChildPath $Root $name),(Get-ChildPath $Stage ($name+'.failed')))
        }
        throw
    }
}

function Invoke-PackageUpdate {
    $root = $PSScriptRoot
    $exe = Get-ChildPath $root 'OkColorPicker.exe'
    $parent = Get-Process -Id $ParentId
    if ($parent.Path -ine $exe) { throw 'Unexpected parent application.' }
    $cache = Get-ChildPath $root '.okcolor-update'
    [void][IO.Directory]::CreateDirectory($cache)
    try { $lock = [IO.File]::Open((Get-ChildPath $cache 'update.lock'),'OpenOrCreate','ReadWrite','None') }
    catch [IO.IOException] { [Console]::WriteLine('CURRENT'); return }
    $stage = $null
    $closed = $false
    $keepStage = $false
    try {
        $pending = Get-ChildPath $cache 'pending.json'
        $archive = Get-ChildPath $cache 'release.zip'
        $deferred = [IO.File]::Exists($pending)
        if ($deferred) {
            $asset = Get-Content -LiteralPath $pending -Raw -Encoding UTF8 | ConvertFrom-Json
            if ([version]$asset.version -le [version]$CurrentVersion) {
                [IO.File]::Delete($pending); [IO.File]::Delete($archive); $deferred = $false
            }
        }
        if (!$deferred) {
            [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
            try {
                $release = Invoke-RestMethod "https://api.github.com/repos/$repository/releases/latest" -UserAgent 'OkColorPicker-Updater' -TimeoutSec 20
            } catch {
                if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 404) { [Console]::WriteLine('CURRENT'); return }
                throw
            }
            $asset = Get-ReleaseAsset $release ([version]$CurrentVersion)
            if (!$asset) { [Console]::WriteLine('CURRENT'); return }
            $download = Get-ChildPath $cache 'download.tmp'
            try {
                Invoke-WebRequest $asset.url -UseBasicParsing -UserAgent 'OkColorPicker-Updater' -TimeoutSec 120 -OutFile $download
                Assert-Archive $asset $download
                [IO.File]::Delete($archive)
                [IO.File]::Move($download,$archive)
            } finally { [IO.File]::Delete($download) }
        }
        $stage = Get-ChildPath $cache ('stage-'+[guid]::NewGuid().ToString('N'))
        [void][IO.Directory]::CreateDirectory($stage)
        Expand-Package $asset $archive $stage
        if (!$deferred) {
            [Console]::WriteLine('READY')
            $choice = [Console]::ReadLine()
            if ($choice -notin 'NOW','LATER') { return }
            $temp = Get-ChildPath $cache 'pending.tmp'
            [IO.File]::WriteAllText($temp,($asset | ConvertTo-Json -Compress))
            [IO.File]::Move($temp,$pending)
            if ($choice -eq 'LATER') { [Console]::WriteLine('DEFERRED'); return }
        }
        foreach ($other in @(Get-Process -Name OkColorPicker -ErrorAction SilentlyContinue)) {
            if ($other.Id -ne $ParentId -and $other.Path -ieq $exe) { throw 'Close other OkColorPicker windows and retry.' }
        }
        [Console]::WriteLine('CLOSE')
        if (!$parent.WaitForExit(30000)) { throw 'Application did not close.' }
        $closed = $true
        try { Install-Package $root $stage }
        catch {
            # Keep any remaining originals if a second I/O failure interrupted rollback.
            $keepStage = @(Get-ChildItem -LiteralPath $stage -Filter '*.previous').Count -gt 0
            throw
        }
        [IO.File]::Delete($pending)
        [IO.File]::Delete($archive)
    } catch {
        if ($pending) { [IO.File]::Delete($pending) }
        if (!$closed) { throw }
        Add-Type -AssemblyName System.Windows.Forms
        [void][Windows.Forms.MessageBox]::Show($_.Exception.Message,'OkColorPicker update')
    } finally {
        if ($stage -and !$keepStage) {
            foreach ($name in $members) {
                foreach ($suffix in '','.failed','.previous') { [IO.File]::Delete((Get-ChildPath $stage ($name+$suffix))) }
            }
            [IO.Directory]::Delete($stage)
        }
        $lock.Dispose()
        $parent.Dispose()
    }
    if ($closed) { Start-Process -FilePath $exe -WorkingDirectory $root }
}

if ($MyInvocation.InvocationName -ne '.') {
    try { Invoke-PackageUpdate } catch { [Console]::WriteLine('ERROR: '+$_.Exception.Message); exit 1 }
}
