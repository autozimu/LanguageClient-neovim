$version = '0.1.13'
$name = 'languageclient'
$url = "https://github.com/autozimu/LanguageClient-neovim/releases/download/$version/"
$filename = "$name-$version-"

if ($ENV:PROCESSOR_ARCHITECTURE -eq "AMD64") {
    $filename = $filename + 'x86_64'
} else {
    $filename = $filename + 'i686'
}

$filename = $filename + '-pc-windows-gnu.exe'
$url = $url + $filename

$path = "bin\$name.exe"
if (Test-Path "$path") {
    Remove-Item -Force "$path"
}
$downloader = new-object System.Net.WebClient
echo "Downloading $url"
$downloader.DownloadFile($url, $path)
