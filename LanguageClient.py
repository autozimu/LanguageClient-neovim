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

    def call(self, method, params, mid=None):
        content = {
                "jsonrpc": "2.0",
                "method": method,
                "params": params
                }
        if mid is not None:
            content["id"] = mid
        content = json.dumps(content)
        message = (
                "Content-Length: {}\r\n\r\n"
                "{}".format(len(content), content)
                )
        print(content)
        print()
        self.outfile.write(message)
        self.outfile.flush()

    def serve(self):
        while True:
            line = self.infile.readline()
            if line:
                contentLength = int(line.split(":")[1])
                self.infile.readline()
                content = self.infile.read(contentLength)
                print(content)
                print()
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

    def incMid(self) -> int:
        mid = self.mid
        self.mid += 1
        return mid

    def echo(self, message):
        self.nvim.command('echo "{}"'.format(message))

    @neovim.command('LanguageClientInitialize')
    def initialize(self, rootPath: str=None):
        if rootPath is None:
            rootPath = getRootPath(self.nvim.current.buffer.name)

        mid = self.incMid()
        self.queue[mid] = self.handleInitializeResponse

        self.rpc.call('initialize', {
            "processId": os.getpid(),
            "rootPath": rootPath,
            "capabilities":{},
            "trace":"verbose"
            }, mid)

    def handleInitializeResponse(self, result: dict):
        self.capabilities = result['capabilities']
        self.nvim.command('echom "LanguageClient started."')

    @neovim.function('LanguageClient_textDocument_didOpen')
    def textDocument_didOpen(self, filename: str=None):
        if filename is None:
            filename = self.nvim.current.buffer.name

        uri = convertToURI(filename)
        languageId = self.nvim.eval('&filetype')
        with open(filename) as f:
            text = f.read()

        self.rpc.call('textDocument/didOpen', {
            "uri": uri,
            "languageId": languageId,
            "version": 1,
            "text": text
            })

    @neovim.function('LanguageClient_textDocument_hover')
    def textDocument_hover(self, filename: str, line: int, character: int):
       mid = self.incMid()
       self.queue[mid] = self.handleTextDocumentHoverResponse

       self.rpc.call('textDocument/hover', {
           "textDocument": {
               "uri": convertToURI(filename)
               },
           "position": {
               "line": line,
               "character": character
               }
           }, mid)

    def handleTextDocumentHoverResponse(self, result: dict):
        value = ''
        for content in result['contents']:
            value += content['value']
            self.echo(value)

    def textDocument_publishDiagnostics(self, params):
        uri = params['uri']
        for diagnostic in params['diagnostics']:
            source = diagnostic['source']
            severity = diagnostic['severity']
            message = diagnostic['message']
            # TODO: escape speical character
            # self.nvim.command('echom "{}"'.format(message))
            self.nvim.command('echom "Diagnostic message received"')

    def handle(self, message):
        if 'result' in message: # got response
            mid = message['id']
            self.queue[mid](message['result'])
            del self.queue[mid]
        else: # request/notification
            methodname = message['method'].replace('/', '_')
            if hasattr(self, methodname):
                getattr(self, methodname)(message['params'])


def getRootPath(filename: str) -> str:
    if filename.endswith('.rs'):
        return traverseUp(filename, lambda path:
                os.path.exists(os.path.join(path, 'Cargo.toml')))
    # TODO: detect for other filetypes
    else:
        return filename

def traverseUp(path: str, stop) -> str:
    if stop(path):
        return path
    else:
        return traverseUp(os.path.dirname(path), stop)

def test_getRootPath():
    assert getRootPath("/tmp/sample-rs/src/main.rs") == "/tmp/sample-rs"

def convertToURI(filename: str) -> str:
    return "file://" + filename

def test_convertToURI():
    assert convertToURI("/tmp/sample-rs/src/main.rs") == "file:///tmp/sample-rs/src/main.rs"

def test_LanguageClient():
    nvim = neovim.attach('child', argv=['/usr/bin/env', 'nvim', '--embed'])
    client = LanguageClient(nvim)

    # initialize
    client.initialize("/private/tmp/sample-rs")
    while not client.capabilities:
        time.sleep(0.1)
    ## wait for notification
    # time.sleep(300)

    # textDocument/didOpen
    client.textDocument_didOpen("/private/tmp/sample-rs/src/main.rs")

    time.sleep(2)

    # textDocument/hover
    client.textDocument_hover("/private/tmp/sample-rs/src/main.rs", 8, 22)

    while len(client.queue) > 0:
        time.sleep(0.1)

