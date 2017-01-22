import neovim
import os, subprocess
import json
import threading
import time
from functools import partial

class RPC:
    def __init__(self, infile, outfile, handler):
        self.infile = infile
        self.outfile = outfile
        self.handler = handler

    def call(self, method, params, mid=None):
        content = {
                "jsonrpc": "2.0",
                "method": method,
                "params": params
                }
        if mid != None:
            content["id"] = mid
        content = json.dumps(content)
        message = (
                "Content-Length: {}\r\n\r\n"
                "{}".format(len(content), content)
                )
        print(content)
        self.outfile.write(message)
        self.outfile.flush()

    def serve(self):
        while True:
            line = self.infile.readline()
            if line:
                contentLength = int(line.split(":")[1])
                content = self.infile.read(contentLength + 1)
                print(content)
                self.handler.handle(json.loads(content))

@neovim.plugin
class LanguageClient:
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
        threading.Thread(target=self.rpc.serve, name="RPC Server", daemon=True).start()
        self.mid = 0
        self.queue = {}
        self.capabilities = {}

    @neovim.command('LanguageClientInitialize')
    def initialize(self, rootPath=None):
        if not rootPath:
            rootPath = getRootPath(self.nvim.current.buffer.name)

        mid = self.mid
        self.mid += 1
        self.queue[mid] = partial(self.handleInitializeResponse, mid);

        self.rpc.call('initialize', {
            "processId": os.getpid(),
            "rootPath": rootPath,
            "capabilities":{},
            "trace":"verbose"
            }, mid)

    def handleInitializeResponse(self, mid, result):
        del self.queue[mid]
        self.capabilities = result['capabilities']
        self.nvim.command('echo "LanguageClient started."')

    def textDocument_didOpen(self):
        self.rpc.call('textDocument/didOpen', {
            "uri": "file:///private/tmp/sample-rs/src/main.rs",
            "languageId": "rust",
            "version": 1,
            "text": "\n\nfn greet() -> i32 {\n    42\n}\n\nfn main() {\n let a = 1;\n    println!(\"{}\", greet());\n}\n",
            })

    def textDocument_publishDiagnostics(self, params):
        uri = params['uri']
        for diagnostic in params['diagnostics']:
            source = diagnostic['source']
            severity = diagnostic['severity']
            message = diagnostic['message']
            self.nvim.command('echo "{}"'.format(message))

    def handle(self, message):
        if 'result' in message: # got response
            mid = message['id']
            self.queue[mid](message['result'])
        else: # request/notification
            methodname = message['method'].replace('/', '_')
            if hasattr(self, methodname):
                getattr(self, methodname)(message['params'])


def getRootPath(filename: str):
    if filename.endswith('.rs'):
        return traverseUp(filename, lambda path:
                os.path.exists(os.path.join(path, 'Cargo.toml')))
    # TODO: detect for other filetypes
    else:
        return filename

def traverseUp(path: str, stop):
    if stop(path):
        return path
    else:
        return traverseUp(os.path.dirname(path), stop)

def test_LanguageClient():
    nvim = neovim.attach('child', argv=['/usr/bin/env', 'nvim', '--embed'])
    client = LanguageClient(nvim)

    # initialize
    client.initialize("/private/tmp/sample-rs")
    while not client.capabilities:
        time.sleep(0.1)

    # textDocument/didOpen
    client.textDocument_didOpen()

    assert getRootPath("/tmp/sample-rs/src/main.rs") == "/tmp/sample-rs"
