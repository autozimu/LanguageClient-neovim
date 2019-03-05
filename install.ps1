$version = '0.1.141'
$name = 'languageclient'
$url = "https://github.com/autozimu/LanguageClient-neovim/releases/download/$version/$name-$version-"

if ($ENV:PROCESSOR_ARCHITECTURE -eq 'AMD64') {
    $url += 'x86_64'
} else {
    $url += 'i686'
}

$url += '-pc-windows-gnu.exe'

$path = "$PSScriptRoot\bin\$name.exe"
if (Test-Path $path) {
    Remove-Item -Force $path
}
echo "Downloading $url ..."
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
Invoke-WebRequest -Uri $url -OutFile $path
