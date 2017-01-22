import neovim
import os, subprocess
import json
import threading
import time

class RPC:
    def __init__(self, infile, outfile, handler):
        self.infile = infile
        self.outfile = outfile
        self.handler = handler

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
        print('started')
        while True:
            line = self.infile.readline()
            if line:
                contentLength = int(line.split(":")[1])
                content = self.infile.read(contentLength + 1)
                self.handler.handle(content)

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
        self.rpc = RPC(self.server.stdout, self.server.stdin, self)
        threading.Thread(target=self.rpc.serve, name="RPC Server",
                daemon=True).start()

    def handle(self, message):
        print(message)

    @neovim.command('initialize')
    def initialize(self):
        self.rpc.call('initialize', {
            "processId": os.getpid(),
            "rootPath": "/private/tmp/sample-rs",
            "capabilities":{},
            "trace":"verbose"
            })

def test_LanguageServerClient():
    client = LanguageServerClient(None)
    client.initialize()
    time.sleep(3)
