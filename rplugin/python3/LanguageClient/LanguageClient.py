import neovim
import os
import subprocess
import json
import threading
from functools import partial
from typing import List, Dict, Union, Any  # noqa: F401

from . util import getRootPath, pathToURI, uriToPath, escape
from . logger import logger
from . RPC import RPC
from . TextDocumentItem import TextDocumentItem


@neovim.plugin
class LanguageClient:
    _instance = None  # type: LanguageClient

    def __init__(self, nvim):
        logger.info('__init__')
        self.nvim = nvim
        self.server = None
        self.capabilities = {}
        self.rootUri = None
        self.textDocuments = {}  # type: Dict[str, TextDocumentItem]
        type(self)._instance = self
        self.serverCommands = self.nvim.eval(
                "get(g:, 'LanguageClient_serverCommands', {})")

    def asyncCommand(self, cmds: str) -> None:
        self.nvim.async_call(lambda: self.nvim.command(cmds))

    def asyncEcho(self, message: str) -> None:
        message = escape(message)
        self.asyncCommand("echo '{}'".format(message))

    def getArgs(self, argsL: List, keys: List) -> List:
        if len(argsL) == 0:
            args = {}  # type: Dict[str, Any]
        else:
            args = argsL[0]

        cursor = []  # type: List[int]

        res = []
        for k in keys:
            if k == "uri":
                v = args.get("uri") or pathToURI(self.nvim.current.buffer.name)
            elif k == "languageId":
                v = (args.get("languageId") or
                     self.nvim.current.buffer.options['filetype'])
            elif k == "line":
                v = args.get("line")
                if not v:
                    cursor = self.nvim.current.window.cursor
                    v = cursor[0] - 1
            elif k == "character":
                v = args.get("character") or cursor[1]
            elif k == "bufnames":
                v = args.get("bufnames") or [b.name for b in self.nvim.buffers]
            else:
                v = args.get(k)
            res.append(v)

        return res

    def applyChanges(
            self, changes: Dict,
            curPos: Dict, bufnames: List) -> None:
        cmd = "echo ''"
        for uri, edits in changes.items():
            path = uriToPath(uri)
            if path in bufnames:
                action = "buffer"
            else:
                action = "edit"
            cmd += "| {} {}".format(action, path)
            for edit in edits:
                line = edit['range']['start']['line'] + 1
                character = edit['range']['start']['character'] + 1
                newText = edit['newText']
                cmd += "| execute 'normal! {}G{}|cw{}'".format(
                        line, character, newText)
        cmd += "| buffer {} | normal! {}G{}|".format(
                    uriToPath(curPos["uri"]),
                    curPos["line"] + 1,
                    curPos["character"] + 1)
        self.asyncCommand(cmd)

    @neovim.function("LanguageClient_isAlive")
    def alive(self, warn=True) -> bool:
        ret = False
        if self.server is None:
            msg = "Language client is not running. Try :LanguageClientStart"
        elif self.server.poll() is not None:
            msg = "Failed to start language server: {}".format(
                    self.server.stderr.readlines())
            logger.error(msg)
        else:
            ret = True

        if ret is False and warn:
            self.asyncEcho(msg)
        return ret

    @neovim.function("LanguageClient_setLoggingLevel")
    def setLoggingLevel(self, args):
        logger.setLevel({
            "ERROR": 40,
            "WARNING": 30,
            "INFO": 20,
            "DEBUG": 10,
            }[args[0]])

    @neovim.command('LanguageClientStart')
    def start(self) -> None:
        if self.alive(warn=False):
            self.asyncEcho("Language client has already started.")
            return

        logger.info('Begin LanguageClientStart')

        languageId, = self.getArgs([], ["languageId"])
        if languageId not in self.serverCommands:
            msg = "No language server commmand found for type: {}.".format(
                    languageId)
            logger.error(msg)
            self.asyncEcho(msg)
            return
        command = self.serverCommands[languageId]

        self.server = subprocess.Popen(
            # ["/bin/bash", "/tmp/wrapper.sh"],
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            universal_newlines=True)

        self.rpc = RPC(
            self.server.stdout, self.server.stdin,
            self.handleRequestOrNotification,
            self.handleRequestOrNotification,
            self.handleError)
        threading.Thread(
                target=self.rpc.serve, name="RPC Server", daemon=True).start()

        logger.info('End LanguageClientStart')

        self.initialize([])
        self.textDocument_didOpen()

    @neovim.function('LanguageClient_initialize')
    def initialize(self, args: List) -> None:
        # {rootPath?: str, cb?}
        if not self.alive():
            return

        logger.info('Begin initialize')

        rootPath, languageId, cb = self.getArgs(
                args, ["rootPath", "languageId", "cb"])
        if rootPath is None:
            rootPath = getRootPath(self.nvim.current.buffer.name, languageId)
            self.rootUri = pathToURI(rootPath)
        if cb is None:
            cb = self.handleInitializeResponse

        self.rpc.call('initialize', {
            "processId": os.getpid(),
            "rootPath": rootPath,
            "rootUri": self.rootUri,
            "capabilities": {},
            "trace": "verbose"
            }, cb)

    def handleInitializeResponse(self, result: Dict) -> None:
        self.capabilities = result['capabilities']
        logger.info('End initialize')

    @neovim.autocmd('BufReadPost', pattern="*")
    def textDocument_didOpen(self) -> None:
        if not self.alive(warn=False):
            return

        logger.info('textDocument/didOpen')

        uri, languageId = self.getArgs([], ["uri", "languageId"])
        if languageId not in self.serverCommands:
            return
        text = str.join("\n", self.nvim.current.buffer)

        textDocumentItem = TextDocumentItem(uri, languageId, text)
        self.textDocuments[uri] = textDocumentItem

        self.rpc.notify('textDocument/didOpen', {
            "textDocument": textDocumentItem.__dict__
            })

    @neovim.function('LanguageClient_textDocument_didClose')
    def textDocument_didClose(self, args: List) -> None:
        # {uri?: str}
        if not self.alive():
            return

        logger.info('textDocument/didClose')

        uri, = self.getArgs(args, ["uri"])
        del self.textDocuments[uri]

        self.rpc.notify('textDocument/didClose', {
            "textDocument": {
                "uri": uri
                }
            })

    @neovim.function('LanguageClient_textDocument_hover')
    def textDocument_hover(self, args: List) -> None:
        # {uri?: str, line?: int, character?: int, cb?}
        if not self.alive():
            return

        logger.info('Begin textDocument/hover')

        uri, line, character, cb = self.getArgs(
            args, ["uri", "line", "character", "cb"])
        if cb is None:
            cb = self.handleTextDocumentHoverResponse

        self.rpc.call('textDocument/hover', {
            "textDocument": {
                "uri": uri
                },
            "position": {
                "line": line,
                "character": character
                }
            }, cb)

    def markedStringToString(self, s: Any) -> str:
        if isinstance(s, str):
            return s
        else:
            return s["value"]

    def handleTextDocumentHoverResponse(self, result: Dict) -> None:
        contents = result["contents"]
        value = ''
        if isinstance(contents, list):
            for markedString in result['contents']:
                value += self.markedStringToString(markedString)
        else:
            value += self.markedStringToString(contents)
        self.asyncEcho(value)

        logger.info('End textDocument/hover')

    # TODO
    # completionItem/resolve
    # textDocument/signatureHelp
    # textDocument/codeAction

    @neovim.function('LanguageClient_textDocument_definition')
    def textDocument_definition(self, args: List) -> None:
        # {uri?: str, line?: int, character?: int, cb?}
        if not self.alive():
            return

        logger.info('Begin textDocument/definition')

        uri, line, character, bufnames, cb = self.getArgs(
            args, ["uri", "line", "character", "bufnames", "cb"])
        if cb is None:
            cb = partial(
                    self.handleTextDocumentDefinitionResponse,
                    bufnames=bufnames)

        self.rpc.call('textDocument/definition', {
            "textDocument": {
                "uri": uri
                },
            "position": {
                "line": line,
                "character": character
                }
            }, cb)

    def handleTextDocumentDefinitionResponse(
            self, result: List, bufnames: Union[List, Dict]) -> None:
        if isinstance(result, list) and len(result) > 1:
            msg = ("Handling multiple definitions is not implemented yet."
                   " Jumping to first.")
            logger.error(msg)
            self.asyncEcho(msg)

        if isinstance(result, list):
            defn = result[0]
        else:
            defn = result
        path = uriToPath(defn["uri"])
        if path in bufnames:
            action = "buffer"
        else:
            action = "edit"
        cmd = "{} {}".format(action, path)
        line = defn['range']['start']['line'] + 1
        character = defn['range']['start']['character'] + 1
        cmd += "| normal! {}G{}|".format(line, character)
        self.asyncCommand(cmd)

        logger.info('End textDocument/definition')

    @neovim.function('LanguageClient_textDocument_rename')
    def textDocument_rename(self, args: List) -> None:
        # {uri?: str, line?: int, character?: int, newName?: str, cb?}
        if not self.alive():
            return

        logger.info('Begin textDocument/rename')

        uri, line, character, newName, bufnames, cb = self.getArgs(
            args, ["uri", "line", "character", "newName", "bufnames", "cb"])
        if newName is None:
            self.nvim.call("inputsave")
            newName = self.nvim.call("input", "Rename to: ")
            self.nvim.call("inputrestore")
        if cb is None:
            cb = partial(
                    self.handleTextDocumentRenameResponse,
                    curPos={"line": line, "character": character, "uri": uri},
                    bufnames=bufnames)

        self.rpc.call('textDocument/rename', {
            "textDocument": {
                "uri": uri
                },
            "position": {
                "line": line,
                "character": character,
                },
            "newName": newName
            }, cb)

    def handleTextDocumentRenameResponse(
            self, result: Dict,
            curPos: Dict, bufnames: List) -> None:
        changes = result['changes']
        self.applyChanges(changes, curPos, bufnames)
        logger.info('End textDocument/rename')

    @neovim.function('LanguageClient_textDocument_documentSymbol')
    def textDocument_documentSymbol(self, args: List) -> None:
        # {uri?: str, cb?}
        if not self.alive():
            return

        logger.info('Begin textDocument/documentSymbol')

        uri, cb = self.getArgs(args, ["uri", "cb"])
        if cb is None:
            if self.nvim.eval("get(g:, 'loaded_fzf', 0)") == 1:
                cb = self.handleTextDocumentDocumentSymbolResponse
            else:
                logger.warn("FZF not loaded.")

        self.rpc.call('textDocument/documentSymbol', {
            "textDocument": {
                "uri": uri
                }
            }, cb)

    def fzf(self, source: List, sink: str) -> None:
        self.asyncCommand("""
call fzf#run(fzf#wrap({{
    'source': {},
    'sink': function('{}')
    }}))
""".replace("\n", "").format(json.dumps(source), sink))
        self.nvim.async_call(lambda: self.nvim.feedkeys("i"))

    def handleTextDocumentDocumentSymbolResponse(self, symbols: List) -> None:
        source = []
        for sb in symbols:
            name = sb["name"]
            start = sb["location"]["range"]["start"]
            line = start["line"] + 1
            character = start["character"] + 1
            entry = "{}:{}:\t{}".format(line, character, name)
            source.append(entry)
        self.fzf(source, "LanguageClient#FZFSinkTextDocumentDocumentSymbol")
        logger.info('End textDocument/documentSymbol')

    @neovim.function('LanguageClient_FZFSinkTextDocumentDocumentSymbol')
    def fzfSinkTextDocumentDocumentSymbol(self, args: List) -> None:
        splitted = args[0].split(":")
        line = splitted[0]
        character = splitted[1]
        self.asyncCommand("normal! {}G{}|".format(line, character))

    @neovim.function('LanguageClient_workspace_symbol')
    def workspace_symbol(self, args: List) -> None:
        if not self.alive():
            return
        logger.info("Begin workspace/symbol")

        query, cb = self.getArgs(args, ["query", "cb"])
        if query is None:
            query = ""
        if cb is None:
            cb = self.handleWorkspaceSymbolResponse

        self.rpc.call('workspace/symbol', {
            "query": query
            }, cb)

    def handleWorkspaceSymbolResponse(self, symbols: list) -> None:
        source = []
        for sb in symbols:
            path = os.path.relpath(sb["location"]["uri"], self.rootUri)
            start = sb["location"]["range"]["start"]
            line = start["line"] + 1
            character = start["character"] + 1
            name = sb["name"]
            entry = "{}:{}:{}\t{}".format(path, line, character, name)
            source.append(entry)
        self.fzf(source, "LanguageClient#FZFSinkWorkspaceSymbol")
        logger.info("Begin workspace/symbol")

    @neovim.function("LanguageClient_FZFSinkWorkspaceSymbol")
    def fzfSinkWorkspaceSymbol(self, args: List):
        bufnames, = self.getArgs([], ["bufnames"])

        splitted = args[0].split(":")
        path = uriToPath(os.path.join(self.rootUri, splitted[0]))
        line = splitted[1]
        character = splitted[2]

        if path in bufnames:
            action = "buffer"
        else:
            action = "edit"
        cmd = "{} {}".format(action, path)
        cmd += "| normal! {}G{}|".format(line, character)
        self.asyncCommand(cmd)

    @neovim.function('LanguageClient_textDocument_references')
    def textDocument_references(self, args: List) -> None:
        if not self.alive():
            return
        logger.info("Begin textDocument/references")

        uri, line, character, cb = self.getArgs(
                args, ["uri", "line", "character", "cb"])
        if cb is None:
            cb = self.handleTextDocumentReferencesResponse

        self.rpc.call('textDocument/references', {
            "textDocument": {
                "uri": uri,
                },
            "position": {
                "line": line,
                "character": character,
                },
            "context": {
                "includeDeclaration": True,
                },
            }, cb)

    def handleTextDocumentReferencesResponse(self, locations: List) -> None:
        source = []  # type: List[str]
        for loc in locations:
            path = os.path.relpath(loc["uri"], self.rootUri)
            start = loc["range"]["start"]
            line = start["line"] + 1
            character = start["character"] + 1
            entry = "{}:{}:{}".format(path, line, character)
            source.append(entry)
        self.fzf(source, "LanguageClient#FZFSinkTextDocumentReferences")
        logger.info("End textDocument/references")

    @neovim.function("LanguageClient_FZFSinkTextDocumentReferences")
    def fzfSinkTextDocumentReferences(self, args: List) -> None:
        bufnames, = self.getArgs([], ["bufnames"])

        splitted = args[0].split(":")
        path = uriToPath(os.path.join(self.rootUri, splitted[0]))
        line = splitted[1]
        character = splitted[2]

        if path in bufnames:
            action = "buffer"
        else:
            action = "edit"
        cmd = "{} {}".format(action, path)
        cmd += "| normal! {}G{}|".format(line, character)
        self.asyncCommand(cmd)

    @neovim.autocmd("TextChanged", pattern="*")
    def textDocument_autocmdTextChanged(self):
        self.textDocument_didChange()

    @neovim.autocmd("TextChangedI", pattern="*")
    def textDocument_autocmdTextChangedI(self):
        self.textDocument_didChange()

    def textDocument_didChange(self) -> None:
        if not self.alive(warn=False):
            return
        logger.info("textDocument/didChange")

        uri, languageId = self.getArgs([], ["uri", "languageId"])
        if not uri or languageId not in self.serverCommands:
            return
        if uri not in self.textDocuments:
            self.textDocument_didOpen()
        newText = str.join("\n", self.nvim.current.buffer)
        version, changes = self.textDocuments[uri].change(newText)

        self.rpc.notify("textDocument/didChange", {
            "textDocument": {
                "uri": uri,
                "version": version
                },
            "contentChanges": changes
            })

    @neovim.autocmd("BufWritePost", pattern="*")
    def textDocument_didSave(self) -> None:
        if not self.alive(warn=False):
            return
        logger.info("textDocument/didSave")

        uri, languageId = self.getArgs([], ["uri", "languageId"])
        if languageId not in self.serverCommands:
            return

        self.rpc.notify("textDocument/didSave", {
            "textDocument": {
                "uri": uri
                }
            })

    @neovim.function("LanguageClient_textDocument_completion")
    def textDocument_completion(self, args: List) -> List:
        if not self.alive():
            return []
        logger.info("Begin textDocument/completion")

        uri, line, character = self.getArgs(args, ["uri", "line", "character"])

        items = self.rpc.call('textDocument/completion', {
            "textDocument": {
                "uri": uri
                },
            "position": {
                "line": line,
                "character": character
                }
            })

        if isinstance(items, dict):  # CompletionList object
            items = items["items"]

        logger.info("End textDocument/completion")
        return items

    # FIXME: python infinite loop after this call.
    @neovim.function("LanguageClient_exit")
    def exit(self, args: List) -> None:
        # {uri?: str}
        if not self.alive():
            return
        logger.info("exit")

        self.rpc.notify("exit", {})

    def textDocument_publishDiagnostics(self, params) -> None:
        for diagnostic in params['diagnostics']:
            # TODO: integration with ale.
            message = diagnostic['message'].replace("\n", ". ")  # noqa: F841
            # self.asyncEcho(message)

    def handleRequestOrNotification(self, message) -> None:
        method = message['method'].replace('/', '_')
        if hasattr(self, method):
            try:
                getattr(self, method)(message['params'])
            except:
                logger.exception("Exception in handle.")
        else:
            logger.warn('no handler implemented for ' + method)

    def handleError(self, message) -> None:
        self.asyncEcho(json.dumps(message))
