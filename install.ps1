$version = '0.1.0'
$name = 'languageclient'
$url = "https://github.com/autozimu/LanguageClient-neovim/releases/download/$version/"
$filename = "$name-$version-"

if ($ENV:PROCESSOR_ARCHITECTURE -eq "AMD64") {
    $filename = $filename + 'x86_64'
} else {
    $filename = $filename + 'i686'
}

$filename = $filename + '-pc-windows-msvc.zip'
$url = $url + $filename

$dir = "$env:TEMP"
$path = "$dir\$filename"
if (Test-Path $path) {
    Remove-Item -Force $path
}
$downloader = new-object System.Net.WebClient
echo "Downloading $url"
$downloader.DownloadFile($url, $path)

if (Test-Path "bin\$name.exe") {
    Remove-Item -Force "bin\$name.exe"
}
echo "Extracting to bin\$name"
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::ExtractToDirectory("$path", "bin")
