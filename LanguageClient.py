import neovim
import os, subprocess
import json
import threading
import time
from functools import partial

from util import getRootPath, convertToURI
from logger import logger

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
        logger.info(content)
        self.outfile.write(message)
        self.outfile.flush()

    def serve(self):
        while True:
            line = self.infile.readline()
            if line:
                contentLength = int(line.split(":")[1])
                self.infile.readline()
                content = self.infile.read(contentLength)
                logger.info(content)
                self.handler.handle(json.loads(content))

@neovim.plugin
class LanguageClient:
    def __init__(self, nvim):
        logger.info('class init')
        self.nvim = nvim
        self.server = None
        self.mid = 0
        self.queue = {}
        self.capabilities = {}

    def incMid(self) -> int:
        mid = self.mid
        self.mid += 1
        return mid

    def echo(self, message):
        self.nvim.async_call(lambda:
                self.nvim.command("echom '{}'".format(
                    message.replace("'", "''"))))

    def alive(self) -> bool:
        if self.server == None:
            return False
        if self.server.poll() != None:
            self.server = None
            return False
        return True

    @neovim.command('LanguageClientStart')
    def start(self):
        logger.info('start')


        if self.alive():
            return

        self.server = subprocess.Popen(
            # ["/bin/bash", "/opt/rls/wrapper.sh"],
            ["cargo", "run", "--manifest-path=/opt/rls/Cargo.toml"],
            # ['langserver-go', '-trace', '-logfile', '/tmp/langserver-go.log'],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            universal_newlines=True)
        self.rpc = RPC(self.server.stdout, self.server.stdin, self)
        threading.Thread(target=self.rpc.serve, name="RPC Server", daemon=True).start()

    @neovim.function('LanguageClient_initialize')
    def initialize(self, args, cb=None):
        logger.info('initialize')

        if not self.alive():
            return

        if len(args) == 0:
            rootPath = getRootPath(self.nvim.current.buffer.name)
        else:
            rootPath = args[0]

        mid = self.incMid()
        self.queue[mid] = partial(self.handleInitializeResponse, cb=cb)

        self.rpc.call('initialize', {
            "processId": os.getpid(),
            "rootPath": rootPath,
            "rootUri": convertToURI(rootPath),
            "capabilities":{},
            "trace":"verbose"
            }, mid)

    def handleInitializeResponse(self, result: dict, cb):
        self.capabilities = result['capabilities']
        self.echo("LanguageClient started.")
        if cb is not None:
            cb(result)

    @neovim.function('LanguageClient_textDocument_didOpen')
    def textDocument_didOpen(self, args):
        logger.info('textDocument/didOpen')

        if not self.alive():
            return

        if len(args) == 0:
            filename = self.nvim.current.buffer.name
        else:
            filename = args[0]


        uri = convertToURI(filename)
        languageId = self.nvim.eval('&filetype')

        self.rpc.call('textDocument/didOpen', {
            "uri": uri,
            "languageId": languageId,
            "version": 1,
            })

    @neovim.function('LanguageClient_textDocument_hover')
    def textDocument_hover(self, args, cb=None):
        logger.info('textDocument/hover')

        if not self.alive():
            return

        if len(args) == 0:
            filename = self.nvim.current.buffer.name
            # vim start with 1
            line = self.nvim.eval("line('.')") - 1
            character = self.nvim.eval("col('.')")
        else:
            filename, line, character = args

        mid = self.incMid()
        self.queue[mid] = partial(self.handleTextDocumentHoverResponse, cb=cb)

        self.rpc.call('textDocument/hover', {
            "textDocument": {
                "uri": convertToURI(filename)
                },
            "position": {
                "line": line,
                "character": character
                }
            }, mid)

    def handleTextDocumentHoverResponse(self, result: dict, cb):
        value = ''
        for content in result['contents']:
            value += content['value']
        self.echo(value)
        if cb is not None:
            cb(value)

    #TODO
    # textDocument/didChange
    # textDocument/didSave
    # textDocument/didClose
    # textDocument/completion
    # completionItem/resolve
    # textDocument/signatureHelp
    # textDocument/definition
    # textDocument/references
    # textDocument/rename
    # textDocument/documentSymbol
    # workspace/symbol
    # textDocument/codeAction

    def textDocument_publishDiagnostics(self, params):
        uri = params['uri']
        for diagnostic in params['diagnostics']:
            source = diagnostic['source']
            severity = diagnostic['severity']
            message = diagnostic['message']
            self.echo(message)

    def handle(self, message):
        if 'result' in message: # got response
            mid = message['id']
            self.queue[mid](message['result'])
            del self.queue[mid]
        else: # request/notification
            methodname = message['method'].replace('/', '_')
            if hasattr(self, methodname):
                getattr(self, methodname)(message['params'])
            else:
                logger.warn('no handler implemented for ' + methodname)
