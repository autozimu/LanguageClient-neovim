import neovim
import os, subprocess
import json
import threading
import time
from functools import partial
from typing import List

from . util import getRootPath, convertToURI
from . logger import logger
from . RPC import RPC

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

    def asyncEcho(self, message):
        message = message.replace("'", "''")
        self.nvim.async_call(lambda:
                self.nvim.command("echom '{}'".format(message)))

    def asyncEval(self, expr):
        expr = expr.replace("'", "''")
        self.nvim.async_call(lambda:
                self.nvim.eval(expr))

    def getPos(self):
        _, line, character, _ = self.nvim.eval("getpos('.')")
        return [line - 1, character - 1]

    def applyChanges(self):
        # TODO
        logger.warn('applyChanges not implemented')

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
        self.asyncEcho("LanguageClient started.")
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
            line, character = self.getPos()
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
        self.asyncEval(value)
        if cb is not None:
            cb(value)

    #TODO
    # textDocument/didChange
    # textDocument/didSave
    # textDocument/didClose
    # textDocument/completion
    # completionItem/resolve
    # textDocument/signatureHelp
    # textDocument/references
    # textDocument/documentSymbol
    # workspace/symbol
    # textDocument/codeAction

    @neovim.function('LanguageClient_textDocument_definition')
    def textDocument_definition(self, args, cb=None):
        logger.info('textDocument/definition')

        if not self.alive():
            return

        if len(args) == 0:
            filename = self.nvim.current.buffer.name
            line, character = self.getPos()
        else:
            filename, line, character = args

        mid = self.incMid()
        self.queue[mid] = partial(self.handleTextDocumentDefinitionResponse, cb=cb)

        self.rpc.call('textDocument/definition', {
            "textDocument": {
                "uri": convertToURI(filename)
                },
            "position": {
                "line": line,
                "character": character
                }
            }, mid)

    def handleTextDocumentDefinitionResponse(self, result: List, cb):
        if len(result) > 1:
            logger.warn("Handling multiple definition are not implemented yet.")

        defn = result[0]
        fileuri = defn['uri']
        line = defn['range']['start']['line'] + 1
        character = defn['range']['start']['character'] + 1
        self.asyncEval("cursor({}, {})".format(line, character))

        if cb is not None:
            cb([line, character])

    @neovim.function('LanguageClient_textDocument_rename')
    def textDocument_rename(self, args, cb=None):
        logger.info('textDocument/rename')

        if not self.alive():
            return

        if len(args) == 1:
            filename = self.nvim.current.buffer.name
            line, character = self.getPos()
            newName = args[0]
        else:
            filename, line, character, newName = args

        mid = self.incMid()
        self.queue[mid] = partial(self.handleTextDocumentRenameResponse, cb=cb)

        self.rpc.call('textDocument/rename', {
            "textDocument": {
                "uri": convertToURI(filename)
                },
            "position": {
                "line": line,
                "character": character,
                },
            "newName": newName
            }, mid)

    def handleTextDocumentRenameResponse(self, result: dict, cb):
        changes = result['changes']
        self.applyChanges(changes)

    @neovim.function('LanguageClient_textDocument_symbol')
    def textDocument_symbol(self, args, cb=None):
        logger.info('textDocument/symbol')

        if not self.alive():
            return

        if len(args) == 0:
            filename = self.nvim.current.buffer.name
        else:
            filename = args[0]

        mid = self.incMid()
        self.queue[mid] = partial(self.handleTextDocumentSymbolResponse, cb=cb)

        self.rpc.call('textDocument/symbol', {
            "textDocument": {
                "uri": convertToURI(filename)
                }
            }, mid)

    def handleTextDocumentSymbolResponse(self, result: List, cb):
        if cb is not None:
            cb(result)

    def textDocument_publishDiagnostics(self, params):
        uri = params['uri']
        for diagnostic in params['diagnostics']:
            source = diagnostic['source']
            severity = diagnostic['severity']
            message = diagnostic['message']
            self.asyncEcho(message)

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
