import neovim
import os
import subprocess
import json
import threading
from functools import partial
from typing import List, Dict, Union, Any  # noqa: F401

from . util import (
        getRootPath, pathToURI, uriToPath, escape,
        getGotoFileCommand)
from . logger import logger
from . RPC import RPC
from . TextDocumentItem import TextDocumentItem
from . DiagnosticsDisplay import DiagnosticsDisplay


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
        self.diagnostics = {}
        self.lastLine = -1
        self.hlsid = None
        self.signid = 0
        type(self)._instance = self
        self.serverCommands = self.nvim.eval(
                "get(g:, 'LanguageClient_serverCommands', {})")

    def asyncCommand(self, cmds: str) -> None:
        self.nvim.async_call(lambda: self.nvim.command(cmds))

    def asyncEcho(self, message: str) -> None:
        message = escape(message)
        self.asyncCommand("echo '{}'".format(message))

    def asyncEchoEllipsis(self, msg: str):
        """
        Print as much of msg as possible without trigging "Press Enter"
        prompt.

        Inspired by neomake, which is in turn inspired by syntastic.
        """
        msg = msg.replace("\n", " ").replace("\t", " ")
        columns = self.nvim.options["columns"]
        if len(msg) > columns - 12:
            msg = msg[:columns - 15] + "..."

        self.asyncEcho(msg)

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
            elif k == "cword":
                v = args.get("cword") or self.nvim.call("expand", "<cword>")
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
            cmd += "| " + getGotoFileCommand(path, bufnames)
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

    @neovim.function("LanguageClient_alive")
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

    def defineSigns(self) -> None:
        diagnosticsDisplay = self.nvim.eval(
                "get(g:, 'LanguageClient_diagnosticsDisplay', {})")
        DiagnosticsDisplay.update(diagnosticsDisplay)
        cmd = "echo ''"
        for level in DiagnosticsDisplay.values():
            name = level["name"]
            signText = level["signText"]
            signTexthl = level["signTexthl"]
            cmd += ("| execute 'sign define LanguageClient{}"
                    " text={} texthl={}'").format(name, signText, signTexthl)
        self.asyncCommand(cmd)

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

        self.defineSigns()

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
            # TODO
            msg = ("Handling multiple definitions is not implemented yet."
                   " Jumping to first.")
            logger.error(msg)
            self.asyncEcho(msg)

        if isinstance(result, list):
            if len(result) == 0:
                self.asyncEcho("Not found.")
                return
            defn = result[0]
        else:
            defn = result
        path = uriToPath(defn["uri"])
        cmd = getGotoFileCommand(path, bufnames)
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

        uri, line, character, cword, newName, bufnames, cb = self.getArgs(
            args, ["uri", "line", "character", "cword", "newName",
                   "bufnames", "cb"])
        if newName is None:
            self.nvim.call("inputsave")
            newName = self.nvim.call("input", "Rename to: ", cword)
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
            cb = partial(self.handleTextDocumentDocumentSymbolResponse,
                         selectionUI=self.getSelectionUI())

        self.rpc.call('textDocument/documentSymbol', {
            "textDocument": {
                "uri": uri
                }
            }, cb)

    def getSelectionUI(self) -> str:
        if self.nvim.eval("get(g:, 'loaded_fzf', 0)") == 1:
            return "fzf"
        return ""

    def fzf(self, source: List, sink: str) -> None:
        self.asyncCommand("""
call fzf#run(fzf#wrap({{
    'source': {},
    'sink': function('{}')
    }}))
""".replace("\n", "").format(json.dumps(source), sink))
        self.nvim.async_call(lambda: self.nvim.feedkeys("i"))

    def handleTextDocumentDocumentSymbolResponse(
            self, symbols: List, selectionUI: str) -> None:
        if selectionUI == "fzf":
            source = []
            for sb in symbols:
                name = sb["name"]
                start = sb["location"]["range"]["start"]
                line = start["line"] + 1
                character = start["character"] + 1
                entry = "{}:{}:\t{}".format(line, character, name)
                source.append(entry)
            self.fzf(source,
                     "LanguageClient#FZFSinkTextDocumentDocumentSymbol")
        else:
            msg = "No selection UI found. Consider install fzf or denite.vim."
            self.asyncEcho(msg)
            logger.warn(msg)

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
        logger.info("End workspace/symbol")

    @neovim.function("LanguageClient_FZFSinkWorkspaceSymbol")
    def fzfSinkWorkspaceSymbol(self, args: List):
        bufnames, = self.getArgs([], ["bufnames"])

        splitted = args[0].split(":")
        path = uriToPath(os.path.join(self.rootUri, splitted[0]))
        line = splitted[1]
        character = splitted[2]

        cmd = getGotoFileCommand(path, bufnames)
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

        cmd = getGotoFileCommand(path, bufnames)
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
        uri = params["uri"]
        diagnostics = {}
        for entry in params['diagnostics']:
            line = entry["range"]["start"]["line"]
            diagnostics[line] = entry
        self.diagnostics[uri] = diagnostics
        self.nvim.async_call(lambda: self.addHighlightAndSign(params))

    def addHighlightAndSign(self, params):
        uri = params["uri"]
        buf = self.nvim.current.buffer
        if uriToPath(uri) != buf.name:
            return

        if not self.hlsid:
            self.hlsid = self.nvim.new_highlight_source()
        buf.clear_highlight(self.hlsid)
        while self.signid > 0:
            self.nvim.command("sign unplace {}".format(self.signid))
            self.signid -= 1
        for entry in params["diagnostics"]:
            line = entry["range"]["start"]["line"]
            start = entry["range"]["start"]["character"]
            end = entry["range"]["end"]["character"]
            display = DiagnosticsDisplay[entry.get("severity", 3)]
            texthl = display["texthl"]
            buf.add_highlight(texthl, line, start, end, self.hlsid)
            name = display["name"]
            self.signid += 1
            self.nvim.command("sign place {} line={}"
                              " name=LanguageClient{} buffer={}".format(
                                self.signid, line + 1, name, buf.number))

    @neovim.autocmd("CursorMoved", pattern="*")
    def showDiagnosticMessage(self) -> None:
        uri, line = self.getArgs([], ["uri", "line"])
        if not uri or line == self.lastLine:
            return
        self.lastLine = line

        entry = self.diagnostics.get(uri, {}).get(line)
        if not entry:
            self.asyncEcho("")
            return

        msg = ""
        if "severity" in entry:
            severity = {
                    1: "E",
                    2: "W",
                    3: "I",
                    4: "H",
                    }[entry["severity"]]
            msg += "[{}]".format(severity)
        if "code" in entry:
            code = entry["code"]
            msg += code
        msg += " " + entry["message"]

        self.asyncEchoEllipsis(msg)

    @neovim.function("LanguageClient_completionItem/resolve")
    def completionItem_resolve(self, args: List) -> None:
        if not self.alive():
            return

        logger.info("Begin completionItem/resolve")
        completionItem, cb = self.getArgs(args, ["completionItem", "cb"])
        if cb is None:
            cb = self.handleCompletionItemResolveResponse

        self.rpc.call("completionItem/resolve", completionItem, cb)

    def handleCompletionItemResolveResponse(self, result):
        # TODO: proper integration.
        self.asyncEcho(json.dumps(result))
        logger.info("End completionItem/resolve")

    @neovim.function("LanguageClient_textDocument_signatureHelp")
    def textDocument_signatureHelp(self, args: List):
        if not self.alive():
            return

        logger.info("Begin textDocument/signatureHelp")
        uri, line, character, cb = self.getArgs(
                args,
                ["uri", "line", "character", "cb"])
        if cb is None:
            cb = self.handleTextDocumentSignatureHelpResponse

        self.rpc.call("textDocument/signatureHelp", {
            "textDocument": uri,
            "position": {
                "line": line,
                "character": character,
                }
            }, cb)

    def handleTextDocumentSignatureHelpResponse(self, result):
        # TODO: proper integration.
        self.asyncEcho(json.dumps(result))
        logger.info("End textDocument/signatureHelp")

    def textDocument_codeAction(self, args: List) -> None:
        if not self.alive():
            return

        logger.info("Begin textDocument/codeAction")
        uri, range, context, cb = self.getArgs(
                args,
                ["uri", "range", "context"])
        if cb is None:
            cb = self.handleTextDocumentCodeActionResponse

        self.rpc.call("textDocument/codeAction", {
            "textDocument": uri,
            "range": range,
            "context": context,
            }, cb)

    def handleTextDocumentCodeActionResponse(self, result):
        # TODO: proper integration.
        self.asyncEcho(json.dumps(result))
        logger.info("End textDocument/codeAction")

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
