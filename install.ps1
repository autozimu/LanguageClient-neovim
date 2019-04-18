#!/usr/bin/env pwsh

$version = '0.1.145'
$name = 'languageclient'
$url = "https://github.com/autozimu/LanguageClient-neovim/releases/download/$version/$name-$version-"

if ([Environment]::Is64BitOperatingSystem) {
    $url += 'x86_64'
} else {
    $url += 'i686'
}

$path = "$PSScriptRoot\bin\$name"
$url += switch ($true) {
    $IsMacOS { "-apple-darwin" }
    $IsLinux { "-unknown-linux-musl" }
    Default {
        # Windows
        $path += ".exe"
        "-pc-windows-gnu.exe"
    }
}

if (Test-Path -LiteralPath $path) {
    Remove-Item -Force -LiteralPath $path
}

echo "Downloading $url ..."

if(!$IsCoreCLR) {
    # We only need to do this for Windows PowerShell
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
}

Invoke-WebRequest -Uri $url -OutFile $path
