import neovim
import os, subprocess
import json

@neovim.plugin
class LanguageServerClient:

    def __init__(self, nvim):
        self.nvim = nvim
        self.server = subprocess.Popen(
            ["cargo", "run", "--manifest-path=/opt/rls/Cargo.toml"],
            # ['langserver-go', '-trace', '-logfile', '/tmp/langserver-go.log'],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE)

    @neovim.command('GetDocumentation')
    def GetDocumentation(self):
        MESSAGE = {
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": {
                    "processId": os.getpid(),
                    "rootPath": "/private/tmp/sample-rs",
                    "capabilities":{},
                    "trace":"verbose"
                    }
                }
        body = json.dumps(MESSAGE)
        response = (
                "Content-Length: {}\r\n\r\n"
                "{}".format(len(body), body)
                )

        self.server.stdin.write(response.encode('utf-8'))
        self.server.stdin.flush()

        while True:
            line = self.server.stdout.readline().decode('utf-8')
            if line:
                print(line)
                break

def test_LanguageServerClient():
    client = LanguageServerClient(None)
    client.GetDocumentation()
