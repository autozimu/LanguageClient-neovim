import neovim
import os, subprocess
import json

@neovim.plugin
class LanguageServerClient:

    def __init__(self, nvim):
        self.nvim = nvim
        self.server = subprocess.Popen(
            ["/bin/bash", "/opt/rls/wrapper.sh"],
            # ["cargo", "run", "--manifest-path=/opt/rls/Cargo.toml"],
            # ['langserver-go', '-trace', '-logfile', '/tmp/langserver-go.log'],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            universal_newlines=True)

    def rpc(self, method, params):
        payload = {
                "jsonrpc": "2.0",
                "id": 0,
                "method": method,
                "params": params
                }
        payload = json.dumps(payload)
        message = (
                "Content-Length: {}\r\n\r\n"
                "{}".format(len(payload), payload)
                )
        self.server.stdin.write(message)
        self.server.stdin.flush()

    @neovim.command('GetDocumentation')
    def GetDocumentation(self):
        self.rpc('initialize', {
            "processId": os.getpid(),
            "rootPath": "/private/tmp/sample-rs",
            "capabilities":{},
            "trace":"verbose"
            })

        while True:
            line = self.server.stdout.readline()
            if line:
                print(line)
                break

def test_LanguageServerClient():
    client = LanguageServerClient(None)
    client.GetDocumentation()
