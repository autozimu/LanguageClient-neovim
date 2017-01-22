import neovim
import os, subprocess
import json

class RPC:
    def __init__(self, infile, outfile):
        self.infile = infile
        self.outfile = outfile

    def call(self, method, params):
        content = {
                "jsonrpc": "2.0",
                "id": 0,
                "method": method,
                "params": params
                }
        content = json.dumps(content)
        message = (
                "Content-Length: {}\r\n\r\n"
                "{}".format(len(content), content)
                )
        self.outfile.write(message)
        self.outfile.flush()

    def serve(self):
        while True:
            line = self.infile.readline()
            if line:
                contentLength = int(line.split(":")[1])
                content = self.infile.read(contentLength)
                print(content)

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
        self.rpc = RPC(self.server.stdout, self.server.stdin)

    @neovim.command('GetDocumentation')
    def GetDocumentation(self):
        self.rpc.call('initialize', {
            "processId": os.getpid(),
            "rootPath": "/private/tmp/sample-rs",
            "capabilities":{},
            "trace":"verbose"
            })
        self.rpc.serve()

def test_LanguageServerClient():
    client = LanguageServerClient(None)
    client.GetDocumentation()
