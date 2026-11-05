param([int]$Port = 9229)
$ErrorActionPreference = 'Stop'
$edge = "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe"
if (!(Test-Path $edge)) { throw 'Microsoft Edge is required for these native-browser tests.' }
if (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue) { throw "Port $Port is already in use." }
$profile = Join-Path $env:LOCALAPPDATA "Temp\opencode\liquid-history-tests-$([guid]::NewGuid().ToString('N'))"
$url = ([uri](Join-Path $PSScriptRoot 'history.html')).AbsoluteUri
$process = Start-Process $edge -ArgumentList '--headless','--disable-gpu','--no-first-run','--no-default-browser-check','--allow-file-access-from-files',"--remote-debugging-port=$Port", "--user-data-dir=`"$profile`"", $url -PassThru
$socket = New-Object System.Net.WebSockets.ClientWebSocket
$cancel = New-Object System.Threading.CancellationTokenSource
$cancel.CancelAfter(20000)
try {
    # One startup allowance, then await the test's promise (no timer-based test polling).
    Start-Sleep -Seconds 2
    $target = (Invoke-RestMethod "http://127.0.0.1:$Port/json/list") | Where-Object { $_.type -eq 'page' -and $_.url -eq $url } | Select-Object -First 1
    if (!$target) { throw 'Test browser page was not created.' }
    [void]$socket.ConnectAsync([uri]$target.webSocketDebuggerUrl, $cancel.Token).GetAwaiter().GetResult()
    $request = @{id=1;method='Runtime.evaluate';params=@{expression='window.historyTestDone.then(() => document.getElementById("result").textContent)';awaitPromise=$true;returnByValue=$true}} | ConvertTo-Json -Depth 6 -Compress
    $bytes = [Text.Encoding]::UTF8.GetBytes($request)
    [void]$socket.SendAsync([ArraySegment[byte]]::new($bytes), [Net.WebSockets.WebSocketMessageType]::Text, $true, $cancel.Token).GetAwaiter().GetResult()
    do {
        $buffer = New-Object byte[] 65536
        $message = New-Object Text.StringBuilder
        do {
            $received = $socket.ReceiveAsync([ArraySegment[byte]]::new($buffer), $cancel.Token).GetAwaiter().GetResult()
            [void]$message.Append([Text.Encoding]::UTF8.GetString($buffer, 0, $received.Count))
        } while (!$received.EndOfMessage)
        $response = $message.ToString() | ConvertFrom-Json
    } while ($response.id -ne 1)
    $text = $response.result.result.value
    if (!$text -or !$text.StartsWith('PASS:')) { throw ($response | ConvertTo-Json -Depth 8) }
    Write-Output $text
} finally {
    $socket.Dispose()
    $cancel.Dispose()
    if (!$process.HasExited) { Stop-Process -Id $process.Id }
}
