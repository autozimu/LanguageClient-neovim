import neovim
import os, subprocess
import json
import threading
import time
from functools import partial
from typing import List

from . util import getRootPath, convertToURI, escape
from . logger import logger
from . RPC import RPC

@neovim.plugin
class LanguageClient:
    def __init__(self, nvim):
        logger.info('__init__')
        self.nvim = nvim
        self.server = None
        self.capabilities = {}

    def asyncEval(self, expr):
        self.nvim.async_call(lambda:
                self.nvim.eval(expr))

    def asyncCommand(self, cmds):
        self.nvim.async_call(lambda:
                self.nvim.command(cmds))

    def asyncEcho(self, message):
        message = escape(message)
        self.asyncCommand("echo '{}'".format(message))

    def getPos(self):
        _, line, character, _ = self.nvim.eval("getpos('.')")
        return [line - 1, character - 1]

    def getArgs(self, args: dict, keys: List) -> List:
        if len(args) == 0:
            args = {}
        else:
            args = args[0]

        pos = []

        res = []
        for k in keys:
            if k == "filename":
                v = args.get("filename", self.nvim.current.buffer.name)
            elif k == "line":
                pos = self.getPos()
                v = args.get("line", pos[0])
            elif k == "character":
                v = args.get("character", pos[1])
            else:
                v = args.get(k, None)
            res.append(v)

        if len(res) == 1:
            return res[0]
        else:
            return res

    def applyChanges(self, changes: dict, curPos: List):
        for uri, edits in changes.items():
            for edit in edits:
                line = edit['range']['start']['line'] + 1
                character = edit['range']['start']['character'] + 1
                newText = edit['newText']
                cmd = "normal! {}G{}|cw{}".format(line, character, newText)
                self.asyncCommand(cmd)
        self.asyncCommand("normal! {}G{}|".format(curPos[0] + 1, curPos[1] + 1))

    def alive(self, warn=True) -> bool:
        if self.server == None:
            if warn: logger.warn("Language server is not started.")
            return False
        if self.server.poll() != None:
            if warn: logger.warn("Language server is not started.")
            self.server = None
            return False
        return True

    @neovim.command('LanguageClientStart')
    def start(self):
        if self.alive(warn=False): return

        logger.info('start')

        self.server = subprocess.Popen(
            # ["/bin/bash", "/opt/rls/wrapper.sh"],
            ["cargo", "run", "--manifest-path=/opt/rls/Cargo.toml"],
            # ['langserver-go', '-trace', '-logfile', '/tmp/langserver-go.log'],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            universal_newlines=True)
        self.rpc = RPC(self.server.stdout, self.server.stdin,
                self.handleRequestOrNotification,
                self.handleRequestOrNotification,
                self.handleError)
        threading.Thread(target=self.rpc.serve, name="RPC Server", daemon=True).start()

    @neovim.function('LanguageClient_initialize')
    def initialize(self, args): # {rootPath?: str, cb?}
        if not self.alive(): return

        logger.info('initialize')

        rootPath, cb = self.getArgs(args, ["rootPath", "cb"])
        if rootPath is None:
            rootPath = getRootPath(self.nvim.current.buffer.name)
        if cb is None:
            cb = self.handleInitializeResponse

        self.rpc.call('initialize', {
            "processId": os.getpid(),
            "rootPath": rootPath,
            "rootUri": convertToURI(rootPath),
            "capabilities":{},
            "trace":"verbose"
            }, cb)

    def handleInitializeResponse(self, result: dict):
        self.capabilities = result['capabilities']
        self.asyncEcho("LanguageClient initialization finished.")

    @neovim.function('LanguageClient_textDocument_didOpen')
    def textDocument_didOpen(self, args): # {filename?: str}
        if not self.alive(): return

        logger.info('textDocument/didOpen')

        filename = self.getArgs(args, ["filename"])
        languageId = self.nvim.eval('&filetype')

        self.rpc.notify('textDocument/didOpen', {
            "uri": convertToURI(filename),
            "languageId": languageId,
            "version": 1,
            })

    @neovim.function('LanguageClient_textDocument_hover')
    def textDocument_hover(self, args):
        # {filename?: str, line?: int, character?: int, cb?}
        if not self.alive(): return

        logger.info('textDocument/hover')

        filename, line, character, cb = self.getArgs(args,
                ["filename", "line", "character", "cb"])
        if cb is None:
            cb = self.handleTextDocumentHoverResponse

        self.rpc.call('textDocument/hover', {
            "textDocument": {
                "uri": convertToURI(filename)
                },
            "position": {
                "line": line,
                "character": character
                }
            }, cb)

    def handleTextDocumentHoverResponse(self, result: dict):
        value = ''
        for content in result['contents']:
            value += content['value']
        self.asyncEcho(value)

    #TODO
    # textDocument/didChange
    # textDocument/didSave
    # textDocument/didClose
    # textDocument/completion
    # completionItem/resolve
    # textDocument/signatureHelp
    # textDocument/references
    # textDocument/codeAction

    @neovim.function('LanguageClient_textDocument_definition')
    def textDocument_definition(self, args):
        # {filename?: str, line?: int, character?: int, cb?}
        if not self.alive(): return

        logger.info('textDocument/definition')

        filename, line, character, cb = self.getArgs(args,
                ["filename", "line", "character", "cb"])
        if cb is None:
            cb = self.handleTextDocumentDefinitionResponse

        self.rpc.call('textDocument/definition', {
            "textDocument": {
                "uri": convertToURI(filename)
                },
            "position": {
                "line": line,
                "character": character
                }
            }, cb)

    def handleTextDocumentDefinitionResponse(self, result: List):
        if len(result) > 1:
            logger.warn("Handling multiple definition are not implemented yet.")

        defn = result[0]
        fileuri = defn['uri']
        line = defn['range']['start']['line'] + 1
        character = defn['range']['start']['character'] + 1
        self.asyncCommand("normal! {}G{}|".format(line, character))

    @neovim.function('LanguageClient_textDocument_rename')
    def textDocument_rename(self, args):
        # {filename?: str, line?: int, character?: int, newName: str, cb?}
        if not self.alive(): return

        logger.info('textDocument/rename')

        filename, line, character, newName, cb = self.getArgs(args,
                ["filename", "line", "character", "newName", "cb"])
        if cb is None:
            cb = partial(self.handleTextDocumentRenameResponse,
                    curPos = [line, character])

        self.rpc.call('textDocument/rename', {
            "textDocument": {
                "uri": convertToURI(filename)
                },
            "position": {
                "line": line,
                "character": character,
                },
            "newName": newName
            }, cb)

    def handleTextDocumentRenameResponse(self, result: dict, curPos: List):
        changes = result['changes']
        self.applyChanges(changes, curPos)

    @neovim.function('LanguageClient_textDocument_documentSymbol')
    def textDocument_documentSymbol(self, args):
        # {filename?: str, cb?}
        if not self.alive(): return

        logger.info('textDocument/documentSymbol')

        filename, cb = self.getArgs(args,
                ["filename", "cb"])
        if cb is None:
            cb = self.handleTextDocumentDocumentSymbolResponse

        self.rpc.call('textDocument/documentSymbol', {
            "textDocument": {
                "uri": convertToURI(filename)
                }
            }, cb)

    def handleTextDocumentDocumentSymbolResponse(self, result: List):
        self.asyncEcho("{} symbols".format(len(result)))

    @neovim.function('LanguageClient_workspace_symbol')
    def workspace_symbol(self, args):
        if not self.alive(): return
        logger.info("workspace/symbol")

        query, cb = self.getArgs(args, ["query", "cb"])
        if cb is None:
            cb = self.handleWorkspaceSymbolResponse

        self.rpc.call('workspace/symbol', {
            "query": query
            }, cb)

    def handleWorkspaceSymbolResponse(self, result: list):
        self.asyncEcho("{} symbols".format(len(result)))

    # TODO: test.
    @neovim.function("LanguageClient_textDocument_didChange")
    def textDocument_didChange(self, args):
        # {filename?: str, contentChanges?: []}
        if not self.alive(): return
        logger.info("textDocument/didChange")

        filename, contentChanges = self.getArgs(args, ["filename", "contentChanges"])

        if contentChanges is None:
            content = str.join("\n", self.nvim.eval("getline(1, '$')"))
            contentChanges = [{
                "text": content
                }]

        self.rpc.notify("textDocument/didChange", {
            "textDocument": {
                "uri": convertToURI(filename)
                },
            "contentChanges": contentChanges
            })

    # TODO: test.
    @neovim.function("LanguageClient_textDocument_didSave")
    def textDocument_didSave(self, args):
        # {filename?: str}
        if not self.alive(): return
        logger.info("textDocument/didSave")

        filename = self.getArgs(args, ["filename"])

        self.rpc.notify("textDocument/didSave", {
            "textDocument": {
                "uri": convertToURI(filename)
                }
            })

    def textDocument_publishDiagnostics(self, params):
        uri = params['uri']
        for diagnostic in params['diagnostics']:
            source = diagnostic['source']
            severity = diagnostic['severity']
            message = diagnostic['message']
            self.asyncEcho(message)

    def handleRequestOrNotification(self, message):
        method = message['method'].replace('/', '_')
        if hasattr(self, method):
            try:
                getattr(self, method)(message['params'])
            except:
                logger.exception("Exception in handle.")
        else:
            logger.warn('no handler implemented for ' + method)

    def handleError(self, message):
        self.asyncEcho(json.dumps(message))

