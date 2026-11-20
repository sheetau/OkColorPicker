# The picker sends one UTF-8 JSON job per line. File paths can contain Unicode.
[Console]::InputEncoding = [Text.UTF8Encoding]::new($false)
$ErrorActionPreference = 'Stop'
$program = $env:OKCOLOR_ADOBE_APP
$script = $env:OKCOLOR_ADOBE_SCRIPT
$application = $null
function Connect-RunningHost {
    $running = Get-Process -Name $program -ErrorAction SilentlyContinue
    if (!$running) { return }
    $ids = @("$program.Application")
    if ($program -eq 'InDesign') {
        $versions = foreach ($process in $running) {
            if ($process.Path -match 'InDesign (20[0-9]{2})') { "InDesign.Application.$($Matches[1])" }
        }
        $curVer = [Microsoft.Win32.Registry]::GetValue('HKEY_CLASSES_ROOT\InDesign.Application\CurVer','','')
        $ids = @($versions) + @($curVer) + $ids
    }
    foreach ($id in ($ids | Where-Object { $_ } | Select-Object -Unique)) {
        try { return [Runtime.InteropServices.Marshal]::GetActiveObject($id) } catch {}
        try { return New-Object -ComObject $id } catch {}
    }
    throw 'The running application has no available scripting interface.'
}
while ($null -ne ($line = [Console]::ReadLine())) {
    try {
        if ($null -eq $application) {
            $application = Connect-RunningHost
            if ($null -eq $application) { [Console]::WriteLine('WAIT'); continue }
        }
        $job = ConvertFrom-Json -InputObject $line
        if ($null -ne $job.slots[0] -or $null -ne $job.slots[1] -or $null -ne $job.active -or $job.palette -or $job.swatchFile) {
            $source = "($script)('$program',$line)"
            if ($program -eq 'InDesign') { $result = $application.DoScript($source,1246973031) }
            else { $result = $application.DoJavaScript($source) }
            if ($result -eq 'WAIT') { [Console]::WriteLine('WAIT'); continue }
        } else {
            $null = $application.Name
        }
        [Console]::WriteLine('OK')
    } catch {
        $application = $null
        if (!(Get-Process -Name $program -ErrorAction SilentlyContinue)) { [Console]::WriteLine('WAIT') }
        else { [Console]::WriteLine('ERROR') }
    }
}
