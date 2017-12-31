import inspect
import json
import linecache
import os
import re
import subprocess
import threading
from functools import wraps, partial
from typing import List, Dict, Any, Union  # noqa: F401

import neovim

from .RPC import RPC
from .Sign import Sign
from .TextDocumentItem import TextDocumentItem
from .logger import logger, logpath_server, setLoggingLevel
from .state import (
    state, update_state, execute_command, echo, echomsg, echoerr,
    echo_ellipsis, echo_signature, make_serializable, set_state, alive)
from .util import (
    get_rootPath, path_to_uri, uri_to_path, get_command_goto_file, get_command_update_signs,
    convert_vim_command_args_to_kwargs, apply_TextEdit, markedString_to_str,
    convert_lsp_completion_item_to_vim_style)
from .MessageType import MessageType
from .DiagnosticSeverity import DiagnosticSeverity
from .CommandsClient import CommandsClient


def deco_args(f=None, warn=True):
    """
    Decorate an LSP function such that
    - check if server is alive
    - unify function calls from python and vimscript
    - gather declared parameters

    Decorator pattern of optioanl arguments copied from
    <https://blogs.it.ox.ac.uk/inapickle/2012/01/05/python-decorators-with-optional-arguments/>.
    """
    if f is None:
        # If called without method, we've been called with optional arguments.
        # We return a decorator with the optional arguments filled in.
        # Next time round we'll be decorating method.
        return partial(deco_args, warn=warn)

    @wraps(f)
    def wrapper(*args, **kwargs):
        languageId, = gather_args(["languageId"])
        if not alive(languageId, warn):
            return None

        arg_spec = inspect.getfullargspec(f)
        kwargs_with_defaults = dict(zip(reversed(arg_spec.args),
                                        arg_spec.defaults or ()))
        kwargs_with_defaults.update({
            "self": args[0],
            "languageId": languageId,
        })
        kwargs_with_defaults.update(kwargs)
        try:
            final_args = gather_args(arg_spec.args, args, kwargs_with_defaults)
        except Exception:
            logger.error("Failed to gather_args")
            return None
        return f(*final_args)
    return wrapper


def gather_args(keys: List, args: List = [], kwargs: Dict = {}) -> List:
    """
    Gather needed arguments.
    """
    res = {}  # type: Dict[str, Any]
    for k in keys:
        res[k] = None
    if len(args) > 1 and len(args[1]) > 0:  # from vimscript side
        kwargs.update(args[1][0])
    res.update(kwargs)

    cursor = []  # type: List[int]

    for k in keys:
        if res[k] is not None:
            continue
        elif k == "languageId":
            res[k] = state["nvim"].current.buffer.options["filetype"]
        elif k == "buftype":
            res[k] = state["nvim"].current.buffer.options["buftype"]
        elif k == "uri":
            filename = kwargs.get("filename") or state["nvim"].current.buffer.name
            res[k] = path_to_uri(filename)
        elif k == "line":
            cursor = state["nvim"].current.window.cursor
            res[k] = cursor[0] - 1
        elif k == "character":
            res[k] = cursor[1]
        elif k == "cword":
            res[k] = state["nvim"].funcs.expand("<cword>")
        elif k == "bufnames":
            res[k] = [b.name for b in state["nvim"].buffers]
        elif k == "columns":
            res[k] = state["nvim"].options["columns"]
        else:
            logger.warn("Unknown parameter key: " + k)

    result = [res[k] for k in keys]
    logger.debug("Gathered arguments: {} = {}".format(keys, result))
    return result


def get_selectionUI() -> str:
    """
    Determine selectionUI.
    """
    if state["nvim"].vars.get("loaded_fzf") == 1:
        return "fzf"
    else:
        return "location-list"


def sync_settings() -> None:
    update_state({
        "serverCommands": state["nvim"].vars.get("LanguageClient_serverCommands", {}),
        "changeThreshold": state["nvim"].vars.get("LanguageClient_changeThreshold", 0),
        "selectionUI": state["nvim"].vars.get("LanguageClient_selectionUI") or get_selectionUI(),
        "trace": state["nvim"].vars.get("LanguageClient_trace", "off"),
        "diagnosticsEnable": state["nvim"].vars.get("LanguageClient_diagnosticsEnable", True),
        "diagnosticsList": state["nvim"].vars.get("LanguageClient_diagnosticsList", "quickfix"),
        "autoStart": state["nvim"].vars.get("LanguageClient_autoStart", False),
        "diagnosticsDisplay": state["nvim"].vars.get("LanguageClient_diagnosticsDisplay", {}),
        "settingsPath": state["nvim"].vars.get(
            "LanguageClient_settingsPath",
            os.path.join(".vim", "settings.json")
        ),
        "loadSettings": state["nvim"].vars.get("LanguageClient_loadSettings", False),
    })
    windowLogMessageLevel = state["nvim"].vars.get("LanguageClient_windowLogMessageLevel")
    if windowLogMessageLevel is not None:
        update_state({
            "windowLogMessageLevel": MessageType[windowLogMessageLevel],
        })


def get_current_buffer_text() -> str:
    text = str.join("\n", state["nvim"].current.buffer)
    if state["nvim"].current.buffer.options["endofline"]:
        text += "\n"
    return text


def get_file_line(filepath: str, line: int) -> str:
    modified_buffers = [buffer for buffer in state["nvim"].buffers
                        if buffer.name == filepath and
                        buffer.options["mod"]]

    if len(modified_buffers) == 0:
        return linecache.getline(filepath, line).strip()
    else:
        return modified_buffers[0][line - 1]


def apply_TextDocumentEdit(textDocumentEdit: Dict) -> None:
    """
    Apply a TextDocumentEdit.
    """
    filename = uri_to_path(textDocumentEdit["textDocument"]["uri"])
    edits = textDocumentEdit["edits"]
    # Sort edits. From bottom to top, right to left.
    edits = sorted(reversed(edits), key=lambda edit: (
        -1 * edit["range"]["start"]["line"],
        -1 * edit["range"]["start"]["character"],
    ))
    buffer = next((buffer for buffer in state["nvim"].buffers
                   if buffer.name == filename), None)
    # Open file if needed.
    if buffer is None:
        state["nvim"].command("exe 'edit ' . fnameescape('{}')".format(filename))
        buffer = next((buffer for buffer in state["nvim"].buffers
                       if buffer.name == filename), None)
    text = buffer[:]
    for edit in edits:
        text = apply_TextEdit(text, edit)
    if buffer.options["fixendofline"] and text[-1] == "":
        buffer[:] = text[:-1]
    else:
        buffer[:] = text


def apply_WorkspaceEdit(workspaceEdit: Dict) -> None:
    """
    Apply a WorkspaceEdit.
    """
    logger.info("Begin apply_WorkspaceEdit " + str(workspaceEdit))
    if workspaceEdit.get("documentChanges") is not None:
        for textDocumentEdit in workspaceEdit.get("documentChanges"):
            apply_TextDocumentEdit(textDocumentEdit)
    else:
        for (uri, edits) in workspaceEdit["changes"].items():
            textDocumentEdit = {
                "textDocument": {
                    "uri": uri,
                },
                "edits": edits,
            }
            apply_TextDocumentEdit(textDocumentEdit)


def set_cursor(uri: str, line: int, character: int) -> None:
    """
    Set cursor position.
    """
    cmd = "buffer {} | normal! {}G{}|".format(
        uri_to_path(uri), line + 1, character + 1)
    execute_command(cmd)


def define_signs() -> None:
    """
    Define sign styles.
    """
    cmd = "echo "
    for level in state["diagnosticsDisplay"].values():
        name = level["name"]
        sign_text = level["signText"]
        sign_text_highlight = level["signTexthl"]
        cmd += "| execute 'sign define LanguageClient{} text={} texthl={}'".format(
            name, sign_text, sign_text_highlight)
    execute_command(cmd)


def fzf(source: List, sink: str) -> None:
    """
    Start fzf selection.
    """
    execute_command("""
call fzf#run(fzf#wrap({{
'source': {},
'sink': function('{}')
}}))
""".replace("\n", "").format(json.dumps(source), sink))
    state["nvim"].feedkeys("i")


def show_diagnostics(uri: str, diagnostics: List) -> None:
    """
    Show diagnostics.
    """
    path = uri_to_path(uri)
    buffer = state["nvim"].current.buffer

    if state.get(uri, {}).get("highlight_source_id") is None:
        update_state({
            uri: {
                "highlight_source_id": state["nvim"].new_highlight_source(),
            }
        })
    highlight_source_id = state[uri]["highlight_source_id"]
    buffer.clear_highlight(highlight_source_id)
    signs = []
    qflist = []
    for entry in diagnostics:
        start_line = entry["range"]["start"]["line"]
        start_character = entry["range"]["start"]["character"]
        end_character = entry["range"]["end"]["character"]
        severity = DiagnosticSeverity(entry.get("severity", 3))
        display = state["diagnosticsDisplay"][severity.value]
        text_highlight = display["texthl"]
        buffer.add_highlight(text_highlight, start_line,
                             start_character, end_character,
                             highlight_source_id)

        signs.append(Sign(start_line + 1, severity))

        qflist.append({
            "filename": path,
            "lnum": start_line + 1,
            "col": start_character + 1,
            "nr": entry.get("code"),
            "text": entry["message"],
            "type": DiagnosticSeverity(severity.value).name,
        })

    signs = sorted(set(signs))
    cmd = get_command_update_signs(state[uri].get("signs", []), signs, path)
    execute_command(cmd)
    set_state([uri, "signs"], signs)

    if state["diagnosticsList"] == "quickfix":
        state["nvim"].funcs.setqflist(qflist)
    elif state["diagnosticsList"] == "location":
        state["nvim"].funcs.setloclist(0, qflist)


def show_line_diagnostic(uri: str, line: int, columns: int) -> None:
    logger.info("Begin show_line_diagnostic")
    entry = state.get(uri, {}).get("line_diagnostics", {}).get(line, "")
    if entry == state["last_line_diagnostic"]:
        return

    update_state({
        "last_line_diagnostic": entry,
    })
    echo_ellipsis(entry, columns)


@neovim.plugin
class LanguageClient:
    _instance = None  # type: LanguageClient

    def __init__(self, nvim):
        logger.info("__init__")
        type(self)._instance = self

        self.nvim = nvim
        update_state({
            "nvim": nvim,
        })
        update_state({
            "autoStart": state["nvim"].vars.get("LanguageClient_autoStart", False),
        })

    @neovim.function("LanguageClient_getState", sync=True)
    def getState_vim(self, args: List) -> str:
        """
        Return state object. Skip unserializable parts.

        Note: this function serves only cases that state is needed from
        vimscript. For uses inside python, import state directly.
        """
        state_copy = make_serializable(state)
        return json.dumps(state_copy)

    @neovim.function("LanguageClient_registerServerCommands")
    def registerServerCommands(self, args: List) -> None:
        """
        Add or update serverCommands.
        """
        serverCommands = args[0]  # Dict[str, List[str]]
        update_state({
            "serverCommands": serverCommands
        })

    @neovim.function("LanguageClient_alive", sync=True)
    def alive_vim(self, args: List) -> bool:
        languageId, = gather_args(["languageId"])
        return alive(languageId, warn=False)

    @neovim.function("LanguageClient_setLoggingLevel")
    def setLoggingLevel_vim(self, args: List) -> None:
        setLoggingLevel(args[0])

    @neovim.command("LanguageClientStart", nargs="*", range="")
    def start(self, args=None, warn=True) -> None:
        sync_settings()

        languageId, = gather_args(["languageId"])
        if alive(languageId, warn=False):
            echomsg("Language client has already started.")
            return

        if languageId not in state["serverCommands"]:
            if not warn:
                return
            msg = "No language server command found for type: {}.".format(languageId)
            logger.error(msg)
            echoerr(msg)
            return

        logger.info("Begin LanguageClientStart")

        command = state["serverCommands"][languageId]
        command = [os.path.expandvars(os.path.expanduser(cmd))
                   for cmd in command]

        try:
            proc = subprocess.Popen(
                # ["/bin/bash", "/tmp/wrapper.sh"],
                command,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=open(logpath_server, "wb"))
        except Exception as ex:
            msg = "Failed to start language server: " + ex.args[1]
            logger.exception(msg)
            echoerr(msg)
            return

        rpc = RPC(proc.stdout, proc.stdin, self.handle_request_and_notify, languageId)
        thread = threading.Thread(target=rpc.serve, name="RPC-" + languageId, daemon=True)
        thread.start()

        update_state({
            "servers": {
                languageId: proc,
            },
            "rpcs": {
                languageId: rpc,
            }
        })

        if len(state["servers"]) == 1:
            define_signs()

        # TODO: possibly expand special variables like '%:h'
        kwargs = convert_vim_command_args_to_kwargs(args)
        rootPath = kwargs.get("rootPath")

        logger.info("End LanguageClientStart")

        self.initialize(rootPath=rootPath, languageId=languageId)
        self.textDocument_didOpen(languageId=languageId)
        self.textDocument_didChange(languageId=languageId)

        if state["nvim"].call("exists", "#User#LanguageClientStarted") == 1:
            state["nvim"].command("doautocmd User LanguageClientStarted")

    @neovim.command("LanguageClientStop")
    @deco_args
    def stop(self, languageId: str) -> None:
        self.exit(languageId=languageId)
        update_state({
            "servers": {
                languageId: None
            }
        })

        if state["nvim"].call("exists", "#User#LanguageClientStopped") == 1:
            state["nvim"].command("doautocmd User LanguageClientStopped")

    @neovim.function("LanguageClient_initialize")
    @deco_args
    def initialize(self, rootPath: str, settingsPath: str, languageId: str, handle=True) -> Dict:
        logger.info("Begin initialize")

        if rootPath is None:
            rootPath = get_rootPath(state["nvim"].current.buffer.name, languageId)
        logger.info("rootPath: " + rootPath)
        update_state({
            "rootUris": {
                languageId: path_to_uri(rootPath)
            }
        })

        if settingsPath is None:
            settingsPath = os.path.join(rootPath, state["settingsPath"])
        logger.info("settingsPath: " + settingsPath)

        settings = {}  # type: Dict

        if state["loadSettings"]:
            if os.path.isfile(settingsPath):
                settings = json.load(open(settingsPath))
            else:
                logger.info("settingsPath is not a file")

        result = state["rpcs"][languageId].call("initialize", {
            "processId": os.getpid(),
            "rootPath": rootPath,
            "rootUri": state["rootUris"][languageId],
            "initializationOptions": settings.get("initializationOptions"),
            "capabilities": {
                "workspace": {
                    "applyEdit": True
                },
                "textDocument": {
                    "completion": {
                        "completionItem": {
                            "snippetSupport": True
                        }
                    }
                }
            },
            "trace": state["trace"],
        })

        if result is None or not handle:
            return result

        update_state({
            "capabilities": {
                languageId: result["capabilities"]
            }
        })

        if "initializationOptions" in settings:
            del settings["initializationOptions"]
        self.workspace_didChangeConfiguration(settings=settings, languageId=languageId)
        self.registerCMSource(languageId, result)
        logger.info("End initialize")

        return result

    def registerCMSource(self, languageId: str, result: Dict) -> None:
        completionProvider = result["capabilities"].get("completionProvider")
        if completionProvider is None:
            return

        trigger_patterns = []
        for c in completionProvider.get("triggerCharacters", []):
            trigger_patterns.append(re.escape(c))

        try:
            state["nvim"].call("cm#register_source", dict(
                name="LanguageClient_{}".format(languageId),
                priority=9,
                scopes=[languageId],
                cm_refresh_patterns=trigger_patterns,
                abbreviation="",
                cm_refresh="LanguageClient_completionManager_refresh"))
            logger.info("register completion manager source ok.")
        except Exception as ex:
            logger.warn("register completion manager source failed. Error: " +
                        repr(ex))

    @neovim.autocmd(
        "BufReadPost", pattern="*",
        eval="[{'buftype': &buftype, 'languageId': &filetype, 'filename': expand('%:p')}]")
    def handle_BufReadPost(self, args: List) -> None:
        logger.info("Begin handle BufReadPost")

        buftype, languageId, uri = gather_args(["buftype", "languageId", "uri"], args=args)
        if buftype != "" or not uri:
            return
        # Language server is running but file is not within rootUri.
        if (state["rootUris"].get(languageId) and
                not uri.startswith(state["rootUris"][languageId])):
            return
        # Opened before.
        if state.get(uri, {}).get("textDocument") is not None:
            return

        if alive(languageId, warn=False):
            self.textDocument_didOpen(uri=uri, languageId=languageId)
            show_diagnostics(uri, state.get(uri, {}).get("diagnostics", []))
            line, columns = gather_args(["line", "columns"])
            show_line_diagnostic(uri, line, columns)
        elif state["autoStart"]:
            self.start(warn=False)

        logger.info("End handleBufReadPost")

    @deco_args(warn=False)
    def textDocument_didOpen(self, uri: str, languageId: str) -> None:
        logger.info("Begin textDocument/didOpen")

        text = get_current_buffer_text()

        textDocumentItem = TextDocumentItem(uri, languageId, text)
        set_state([uri, "textDocument"], textDocumentItem)

        state["rpcs"][languageId].notify("textDocument/didOpen", {
            "textDocument": {
                "uri": textDocumentItem.uri,
                "languageId": textDocumentItem.languageId,
                "version": textDocumentItem.version,
                "text": textDocumentItem.text,
            }
        })

        state["nvim"].current.buffer.options["omnifunc"] = "LanguageClient#complete"

        logger.info("End textDocument/didOpen")

    @neovim.function("LanguageClient_textDocument_didClose")
    @deco_args(warn=False)
    def textDocument_didClose(self, uri: str, languageId: str) -> None:
        logger.info("textDocument/didClose")

        state["rpcs"][languageId].notify("textDocument/didClose", {
            "textDocument": {
                "uri": uri
            }
        })

        set_state([uri, "textDocument"], None)

    @deco_args(warn=False)
    def workspace_didChangeConfiguration(self, settings: Dict, languageId: str) -> None:
        logger.info("workspace/didChangeConfiguration")

        state["rpcs"][languageId].notify("workspace/didChangeConfiguration", {
            "settings": settings
        })

    @neovim.function("LanguageClient_workspace_didChangeConfiguration")
    def workspace_didChangeConfiguration_vim(self, args: List) -> None:
        self.workspace_didChangeConfiguration(settings=args[0])

    def _textDocument_hover(self, uri: str, languageId: str,
                            line: int, character: int) -> Dict:
        logger.info("Begin textDocument/hover")

        self.textDocument_didChange()

        result = state["rpcs"][languageId].call("textDocument/hover", {
            "textDocument": {
                "uri": uri
            },
            "position": {
                "line": line,
                "character": character
            }
        })

        logger.info("End textDocument/hover")
        return result

    @neovim.function("LanguageClient_textDocument_hoverSync", sync=True)
    @deco_args
    def textDocument_hoverSync(self, uri: str, languageId: str,
                               line: int, character: int) -> Dict:
        return self._textDocument_hover(uri, languageId, line, character)

    @neovim.function("LanguageClient_textDocument_hover")
    @deco_args
    def textDocument_hover(self, uri: str, languageId: str,
                           line: int, character: int, handle=True) -> Dict:
        result = self._textDocument_hover(uri, languageId, line, character)
        if result is None or not handle:
            return result

        contents = result.get("contents")
        if contents is None:
            contents = "No info."

        if isinstance(contents, list):
            info = str.join("\n", [markedString_to_str(s) for s in contents])
        else:
            info = markedString_to_str(contents)
        echo(info)

        return result

    @neovim.function("LanguageClient_textDocument_definition")
    @deco_args
    def textDocument_definition(
            self, uri: str, languageId: str, line: int, character: int,
            bufnames: List[str], handle=True) -> Union[Dict, List]:
        logger.info("Begin textDocument/definition")

        self.textDocument_didChange()

        result = state["rpcs"][languageId].call("textDocument/definition", {
            "textDocument": {
                "uri": uri
            },
            "position": {
                "line": line,
                "character": character
            }
        })

        if result is None or not handle:
            return result

        if isinstance(result, list) and len(result) > 1:
            # TODO
            msg = ("Handling multiple definitions is not implemented yet."
                   " Jumping to first.")
            logger.error(msg)
            echoerr(msg)

        if isinstance(result, list):
            if len(result) == 0:
                echo("Not found.")
                return result
            defn = result[0]
        else:
            defn = result
        if not defn.get("uri"):
            return None
        if not defn["uri"].startswith("file:///"):
            echo("{}:{}".format(defn["uri"], defn["range"]["start"]["line"]))
            return result
        path = uri_to_path(defn["uri"])
        line = defn["range"]["start"]["line"] + 1
        character = defn["range"]["start"]["character"] + 1
        cmd = get_command_goto_file(path, bufnames, line, character)

        execute_command(cmd)

        logger.info("End textDocument/definition")
        return result

    @neovim.function("LanguageClient_textDocument_rename")
    @deco_args
    def textDocument_rename(
            self, uri: str, languageId: str, line: int, character: int,
            cword: str, newName: str, handle=True) -> Dict:
        logger.info("Begin textDocument/rename")

        self.textDocument_didChange()

        if newName is None:
            state["nvim"].funcs.inputsave()
            newName = state["nvim"].funcs.input("Rename to: ", cword)
            state["nvim"].funcs.inputrestore()

        workspaceEdit = state["rpcs"][languageId].call("textDocument/rename", {
            "textDocument": {
                "uri": uri
            },
            "position": {
                "line": line,
                "character": character,
            },
            "newName": newName
        })

        if workspaceEdit is None or not handle:
            return workspaceEdit

        apply_WorkspaceEdit(workspaceEdit)
        set_cursor(uri, line, character)

        logger.info("End textDocument/rename")
        return workspaceEdit

    @neovim.function("LanguageClient_textDocument_documentSymbol")
    @deco_args
    def textDocument_documentSymbol(self, uri: str, languageId: str, handle=True) -> List:
        logger.info("Begin textDocument/documentSymbol")

        self.textDocument_didChange()

        symbols = state["rpcs"][languageId].call("textDocument/documentSymbol", {
            "textDocument": {
                "uri": uri
            }
        })

        if symbols is None or not handle:
            return symbols

        if state["selectionUI"] == "fzf":
            source = []
            for sb in symbols:
                name = sb["name"]
                start = sb["location"]["range"]["start"]
                line = start["line"] + 1
                character = start["character"] + 1
                entry = "{}:{}:\t{}".format(line, character, name)
                source.append(entry)
            fzf(source, "LanguageClient#FZFSinkTextDocumentDocumentSymbol")
        elif state["selectionUI"] == "location-list":
            loclist = []
            path = uri_to_path(uri)
            for sb in symbols:
                name = sb["name"]
                start = sb["location"]["range"]["start"]
                line = start["line"] + 1
                character = start["character"] + 1
                loclist.append({
                    "filename": path,
                    "lnum": line,
                    "col": character,
                    "text": name,
                })
            state["nvim"].funcs.setloclist(0, loclist)
            echo("Document symbols populated to location list.")
        else:
            msg = "No selection UI found. Consider install fzf or denite.vim."
            logger.warn(msg)
            echoerr(msg)

        logger.info("End textDocument/documentSymbol")
        return symbols

    @neovim.function("LanguageClient_FZFSinkTextDocumentDocumentSymbol")
    def fzfSinkTextDocumentDocumentSymbol(self, args: List) -> None:
        splitted = args[0].split(":")
        line = splitted[0]
        character = splitted[1]
        execute_command("normal! {}G{}|".format(line, character))

    @neovim.function("LanguageClient_workspace_symbol")
    @deco_args
    def workspace_symbol(self, languageId: str, query: str, handle=True) -> List:
        logger.info("Begin workspace/symbol")

        if query is None:
            query = ""

        symbols = state["rpcs"][languageId].call("workspace/symbol", {
            "query": query
        })

        if symbols is None or not handle:
            return symbols

        if state["selectionUI"] == "fzf":
            source = []
            for sb in symbols:
                path = os.path.relpath(sb["location"]["uri"], state["rootUris"][languageId])
                start = sb["location"]["range"]["start"]
                line = start["line"] + 1
                character = start["character"] + 1
                name = sb["name"]
                entry = "{}:{}:{}\t{}".format(path, line, character, name)
                source.append(entry)
            fzf(source, "LanguageClient#FZFSinkWorkspaceSymbol")
        elif state["selectionUI"] == "location-list":
            loclist = []
            for sb in symbols:
                path = uri_to_path(sb["location"]["uri"])
                start = sb["location"]["range"]["start"]
                line = start["line"] + 1
                character = start["character"] + 1
                name = sb["name"]
                loclist.append({
                    "filename": path,
                    "lnum": line,
                    "col": character,
                    "text": name,
                })
            state["nvim"].funcs.setloclist(0, loclist)
            echo("Workspace symbols populated to location list.")
        else:
            msg = "No selection UI found. Consider install fzf or denite.vim."
            logger.warn(msg)
            echoerr(msg)

        logger.info("End workspace/symbol")
        return symbols

    @neovim.function("LanguageClient_FZFSinkWorkspaceSymbol")
    def fzfSinkWorkspaceSymbol(self, args: List):
        bufnames, languageId = gather_args(["bufnames", "languageId"])

        splitted = args[0].split(":")
        path = uri_to_path(os.path.join(state["rootUris"][languageId], splitted[0]))
        line = splitted[1]
        character = splitted[2]

        cmd = get_command_goto_file(path, bufnames, line, character)
        execute_command(cmd)

    @neovim.function("LanguageClient_textDocument_references")
    @deco_args
    def textDocument_references(
            self, uri: str, languageId: str, line: int, character: int,
            includeDeclaration: bool = True, handle=True) -> List:
        logger.info("Begin textDocument/references")

        self.textDocument_didChange()

        locations = state["rpcs"][languageId].call("textDocument/references", {
            "textDocument": {
                "uri": uri,
            },
            "position": {
                "line": line,
                "character": character,
            },
            "context": {
                "includeDeclaration": includeDeclaration,
            },
        })

        if locations is None:
            return locations

        # enhance with the line's contents for Denite
        for loc in locations:
            path = uri_to_path(loc["uri"])
            start = loc["range"]["start"]
            line = start["line"] + 1
            character = start["character"] + 1
            text = get_file_line(path, line)
            loc['text'] = text

        if not handle:
            return locations

        if state["selectionUI"] == "fzf":
            source = []  # type: List[str]
            for loc in locations:
                path = os.path.relpath(loc["uri"],
                                       state["rootUris"][languageId])
                start = loc["range"]["start"]
                line = start["line"] + 1
                character = start["character"] + 1
                text = loc["text"]
                entry = "{}:{}:{}: {}".format(path, line, character, text)
                source.append(entry)
            fzf(source, "LanguageClient#FZFSinkTextDocumentReferences")
        elif state["selectionUI"] == "location-list":
            loclist = []
            for loc in locations:
                path = uri_to_path(loc["uri"])
                start = loc["range"]["start"]
                line = start["line"] + 1
                character = start["character"] + 1
                text = loc["text"]
                loclist.append({
                    "filename": path,
                    "lnum": line,
                    "col": character,
                    "text": text
                })
            state["nvim"].funcs.setloclist(0, loclist)
            echo("References populated to location list.")
        else:
            msg = "No selection UI found. Consider install fzf or denite.vim."
            logger.warn(msg)
            echoerr(msg)

        logger.info("End textDocument/references")
        return locations

    @neovim.function("LanguageClient_rustDocument_implementations")
    @deco_args
    def rustDocument_implementations(
            self, uri: str, languageId: str, line: int, character: int,
            handle=True) -> List:
        logger.info("Begin rustDocument/implementations")

        self.textDocument_didChange()

        locations = state["rpcs"][languageId].call("rustDocument/implementations", {
            "textDocument": {
                "uri": uri,
            },
            "position": {
                "line": line,
                "character": character,
            }
        })

        if locations is None or not handle:
            return locations

        if state["selectionUI"] == "fzf":
            source = []  # type: List[str]
            for loc in locations:
                path = os.path.relpath(loc["uri"],
                                       state["rootUris"][languageId])
                start = loc["range"]["start"]
                line = start["line"] + 1
                character = start["character"] + 1
                text = get_file_line(uri_to_path(loc["uri"]), line)
                entry = "{}:{}:{}: {}".format(path, line, character, text)
                source.append(entry)
            fzf(source, "LanguageClient#FZFSinkTextDocumentReferences")
        elif state["selectionUI"] == "location-list":
            loclist = []
            for loc in locations:
                path = uri_to_path(loc["uri"])
                start = loc["range"]["start"]
                line = start["line"] + 1
                character = start["character"] + 1
                text = get_file_line(path, line)
                loclist.append({
                    "filename": path,
                    "lnum": line,
                    "col": character,
                    "text": text
                })
            state["nvim"].funcs.setloclist(0, loclist)
            echo("References populated to location list.")
        else:
            msg = "No selection UI found. Consider install fzf or denite.vim."
            logger.warn(msg)
            echoerr(msg)

        logger.info("End rustDocument/implementations")
        return locations

    @neovim.function("LanguageClient_FZFSinkTextDocumentReferences")
    def fzfSinkTextDocumentReferences(self, args: List) -> None:
        bufnames, languageId = gather_args(["bufnames", "languageId"])

        splitted = args[0].split(":")
        path = uri_to_path(os.path.join(state["rootUris"][languageId], splitted[0]))
        line = splitted[1]
        character = splitted[2]

        cmd = get_command_goto_file(path, bufnames, line, character)
        execute_command(cmd)

    @neovim.autocmd("TextChanged", pattern="*",
                    eval="[{'filename': expand('%:p'), 'buftype': &buftype}]")
    def handle_TextChanged(self, args: List) -> None:
        logger.info("Begin handle TextChanged")
        uri, buftype = gather_args(["uri", "buftype"], args=args)
        if buftype != "" or state.get(uri, {}).get("textDocument") is None:
            return
        text_doc = state[uri]["textDocument"]
        if text_doc.skip_change(state["changeThreshold"]):
            return
        self.textDocument_didChange()

    @neovim.autocmd("TextChangedI", pattern="*",
                    eval="[{'filename': expand('%:p'), 'buftype': &buftype}]")
    def handle_TextChangedI(self, args: List) -> None:
        logger.info("Begin handle TextChangedI")
        self.handle_TextChanged(args)

    @neovim.function("textDocument_didChange")
    @deco_args(warn=False)
    def textDocument_didChange(self, uri: str, languageId: str) -> None:
        if not uri or languageId not in state["serverCommands"]:
            return
        if state.get(uri, {}).get("textDocument") is None:
            self.textDocument_didOpen()
            return
        new_text = get_current_buffer_text()
        doc = state[uri]["textDocument"]
        if new_text == doc.text:
            return

        logger.info("textDocument/didChange")

        version, changes = doc.change(new_text)

        state["rpcs"][languageId].notify("textDocument/didChange", {
            "textDocument": {
                "uri": uri,
                "version": version
            },
            "contentChanges": changes
        })

        doc.commit_change()

    @neovim.autocmd("BufWritePost", pattern="*",
                    eval="[{'languageId': &filetype, 'filename': expand('%:p')}]")
    def handle_BufWritePost(self, args: List) -> None:
        logger.info("Begin handle BufWritePost")
        uri, languageId = gather_args(["uri", "languageId"], args=args)
        self.textDocument_didSave()

    @neovim.function("textDocument_didSave")
    @deco_args(warn=False)
    def textDocument_didSave(self, uri: str, languageId: str) -> None:
        if languageId not in state["serverCommands"]:
            return

        logger.info("textDocument/didSave")

        state["rpcs"][languageId].notify("textDocument/didSave", {
            "textDocument": {
                "uri": uri
            }
        })

    @neovim.function("LanguageClient_textDocument_completion", sync=True)
    @deco_args(warn=False)
    def textDocument_completion(
            self, uri: str, languageId: str, line: int, character: int) -> Union[List, Dict]:
        logger.info("Begin textDocument/completion")

        self.textDocument_didChange()

        result = state["rpcs"][languageId].call("textDocument/completion", {
            "textDocument": {
                "uri": uri
            },
            "position": {
                "line": line,
                "character": character
            }
        })

        return result

    @neovim.function("LanguageClient_textDocument_completionOmnifunc")
    @deco_args(warn=False)
    def textDocument_completionOmnifunc(self, completeFromColumn: int) -> None:
        result = self.textDocument_completion()
        if result is None:
            items = []  # type: List
        elif isinstance(result, dict):
            items = result["items"]
        else:
            items = result
        matches = [convert_lsp_completion_item_to_vim_style(item) for item in items]
        state["nvim"].funcs.complete(completeFromColumn, matches)

    # this method is called by nvim-completion-manager framework
    @neovim.function("LanguageClient_completionManager_refresh")
    def completionManager_refresh(self, args: List) -> None:
        languageId, = gather_args(["languageId"])
        if not alive(languageId, warn=False):
            return
        logger.info("completionManager_refresh: %s", args)
        info = args[0]
        ctx = args[1]

        if ctx["typed"] == "":
            return

        kwargs = {
            "line": ctx["lnum"] - 1,
            "character": ctx["col"] - 1,
        }

        uri, line, character = gather_args(["uri", "line", "character"], kwargs=kwargs)
        logger.debug("uri[%s] line[%s] character[%s]", uri, line, character)

        result = self.textDocument_completion()
        logger.debug("result: %s", result)

        if result is None:
            return

        items = result
        isIncomplete = False
        if isinstance(result, dict):
            items = result["items"]
            isIncomplete = result.get('isIncomplete', False)

        matches = []
        for item in items:
            match = convert_lsp_completion_item_to_vim_style(item)

            if item.get('additionalTextEdits', None):
                match['additionalTextEdits'] = item['additionalTextEdits']
            if item.get('textEdit'):
                match['textEdit'] = item['textEdit']

            insertText = item.get('insertText', "") or ""
            label = item['label']
            insertTextFormat = item.get('insertTextFormat', 1)

            if insertTextFormat == 2:
                match['word'] = label
                match['snippet'] = insertText
                # When an edit is provided the value of `insertText` is
                # ignored.
                if item.get('textEdit'):
                    # TODO: Not fully conforming to LSP
                    match['snippet'] = item['textEdit']['newText'] + '$0'
            elif item.get('textEdit', None):
                # ignore insertText
                # TODO: Not fully conforming to LSP
                match['word'] = item['textEdit']['newText']
            matches.append(match)

        state["nvim"].call('cm#complete', info['name'], ctx,
                           ctx['startcol'], matches, isIncomplete, async=True)

    @neovim.function("LanguageClient_exit")
    @deco_args
    def exit(self, languageId: str) -> None:
        logger.info("exit")

        state["rpcs"][languageId].notify("exit", {})

    def textDocument_publishDiagnostics(self, diagnostics_params: Dict) -> None:
        if not state["diagnosticsEnable"]:
            return

        uri = diagnostics_params["uri"]
        diagnostics = diagnostics_params["diagnostics"]
        set_state([uri, "diagnostics"], diagnostics)

        line_diagnostics = {}
        for entry in diagnostics:
            line = entry["range"]["start"]["line"]
            msg = ""
            if entry.get("severity"):
                msg += "[{}]".format(DiagnosticSeverity(entry["severity"]).name)
            if entry.get("code"):
                msg += "[]".format(entry["code"])
            msg += " " + entry["message"]
            line_diagnostics[line] = msg

        set_state([uri, "line_diagnostics"], line_diagnostics)

        if path_to_uri(state["nvim"].current.buffer.name) != uri:
            return

        show_diagnostics(uri, diagnostics)

        line, columns = gather_args(["line", "columns"])
        show_line_diagnostic(uri, line, columns)

    @neovim.autocmd("CursorMoved", pattern="*",
                    eval="[{'buftype': &buftype, 'line': line('.')}]")
    def handle_CursorMoved(self, args: List) -> None:
        logger.info("Begin handle CursorMoved")
        buftype, line = gather_args(["buftype", "line"], args=args)
        # Regular file buftype is "".
        if buftype != "" or line == state["last_cursor_line"]:
            return

        update_state({
            "last_cursor_line": line,
        })

        uri, line, columns = gather_args(["uri", "line", "columns"])
        show_line_diagnostic(uri, line, columns)

    @neovim.function("LanguageClient_completionItem/resolve")
    @deco_args
    def completionItem_resolve(
            self, completionItem: Dict, languageId: str, handle=True) -> Dict:
        logger.info("Begin completionItem/resolve")

        self.textDocument_didChange()
        result = state["rpcs"][languageId].call("completionItem/resolve", completionItem)

        if result is None or not handle:
            return result

        # TODO: proper integration.
        logger.warn(result)
        echomsg(json.dumps(result))

        logger.info("End completionItem/resolve")
        return result

    @neovim.function("LanguageClient_textDocument_signatureHelp")
    @deco_args
    def textDocument_signatureHelp(
            self, uri: str, languageId: str, line: int, character: int,
            handle=True) -> Dict:
        logger.info("Begin textDocument/signatureHelp")

        self.textDocument_didChange()
        result = state["rpcs"][languageId].call("textDocument/signatureHelp", {
            "textDocument": {
                "uri": uri,
            },
            "position": {
                "line": line,
                "character": character,
            }
        })

        if result is None or not handle:
            return result

        signatures = result['signatures']
        if len(signatures) == 0:
            return result
        if 'activeSignature' not in result:
            echoerr('No active signature found')
            return result

        activeSignature = signatures[result['activeSignature']]
        if ('activeParameter' not in result or 'parameters' not in activeSignature):
            echo_signature(activeSignature['label'])
            return result

        parameters = activeSignature['parameters']
        activeParamterIdx = result['activeParameter']
        if activeParamterIdx >= len(parameters):
            echo_signature(activeSignature['label'])
            return result
        activeParameter = parameters[activeParamterIdx]
        echo_signature(activeSignature['label'], activeParameter['label'])
        logger.info("End textDocument/signatureHelp")
        return result

    @neovim.function("LanguageClient_textDocument_codeAction")
    @deco_args
    def textDocument_codeAction(
            self, uri: str, languageId: str, line: int, character: int, handle=True) -> Dict:
        logger.info("Begin textDocument/codeAction")

        diagnostics = [entry for entry in state[uri]["diagnostics"]
                       if line >= entry["range"]["start"]["line"] and
                       line <= entry["range"]["end"]["line"] and
                       character >= entry["range"]["start"]["character"] and
                       character <= entry["range"]["end"]["character"]]

        if len(diagnostics) == 0:
            echomsg("No diagnostics found.")
            return None

        range = {
            "start": {
                "line": diagnostics[0]["range"]["start"]["line"],
                "character": diagnostics[0]["range"]["start"]["character"],
            },
            "end": {
                "line": diagnostics[0]["range"]["end"]["line"],
                "character": diagnostics[0]["range"]["end"]["character"],
            },
        }

        self.textDocument_didChange()
        commands = state["rpcs"][languageId].call("textDocument/codeAction", {
            "textDocument": {
                "uri": uri,
            },
            "range": range,
            "context": {
                "diagnostics": diagnostics,
            },
        })

        if commands is None or not handle:
            return commands

        update_state({
            "codeActionCommands": commands,
        })
        source = ["{}: {}".format(entry["command"], entry["title"])
                  for entry in commands]

        if state["selectionUI"] == "fzf":
            fzf(source, "LanguageClient#FZFSinkTextDocumentCodeAction")
        else:
            msg = "No selection UI found. Consider install fzf or denite.vim."
            logger.warn(msg)
            echoerr(msg)

        logger.info("End textDocument/codeAction")
        return commands

    @neovim.function("LanguageClient_FZFSinkTextDocumentCodeAction")
    def fzfSinkTextDocumentCodeAction(self, lines: str) -> None:
        command, title = lines[0].split(": ")
        command = command.strip()
        title = title.strip()
        logger.info("Selected action with command {} title {}".format(
            json.dumps(command), json.dumps(title)))
        entry = next((entry for entry in state["codeActionCommands"]
                      if entry["command"] == command and entry["title"] == title),
                     None)

        if entry is None:
            msg = "Failed to find action entry. Command: {}. Title: {}.".format(
                json.dumps(command), json.dumps(title))
            logger.error(msg)
            echoerr(msg)
            return

        if self.try_handle_command_by_client(entry):
            return

        self.workspace_executeCommand(command=command,
                                      arguments=entry.get("arguments"))
        update_state({
            "codeActionCommands": [],
        })

    def try_handle_command_by_client(self, entry: Dict) -> bool:
        """
        Try handle a Command by client itself.
        """
        try:
            command = CommandsClient(entry["command"])
        except KeyError:
            return False
        except ValueError:
            return False

        if command == CommandsClient.JavaApplyWorkspaceEdit:
            for edit in entry["arguments"]:
                apply_WorkspaceEdit(edit)
        else:
            return False

        return True

    @neovim.function("LanguageClient_workspace_executeCommand")
    @deco_args
    def workspace_executeCommand(self, languageId: str, command: str,
                                 arguments, handle=True):
        logger.info("Begin workspace/executeCommand")

        result = state["rpcs"][languageId].call("workspace/executeCommand", {
            "command": command,
            "arguments": arguments,
        })

        logger.info("End workspace/executeCommand")
        return result

    @neovim.function("LanguageClient_textDocument_formatting")
    @deco_args
    def textDocument_formatting(
            self, languageId: str, uri: str, line: int, character: int,
            handle=True) -> Dict:
        logger.info("Begin textDocument/formatting")

        self.textDocument_didChange()
        options = {
            "tabSize": state["nvim"].current.buffer.options["tabstop"],
            "insertSpaces": state["nvim"].current.buffer.options["expandtab"],
        }
        textEdits = state["rpcs"][languageId].call("textDocument/formatting", {
            "textDocument": {
                "uri": uri,
            },
            "options": options,
        })

        if textEdits is None or not handle:
            return textEdits

        textDocumentEdit = {
            "textDocument": {
                "uri": uri,
            },
            "edits": textEdits,
        }

        apply_TextDocumentEdit(textDocumentEdit)
        set_cursor(uri, line, character)

        logger.info("End textDocument/formatting")
        return textEdits

    @neovim.function("LanguageClient_textDocument_rangeFormatting")
    @deco_args
    def textDocument_rangeFormatting(
            self, languageId: str, uri: str, line: int, character: int,
            handle=True) -> Dict:
        logger.info("Begin textDocument/rangeFormatting")

        self.textDocument_didChange()
        options = {
            "tabSize": state["nvim"].current.buffer.options["tabstop"],
            "insertSpaces": state["nvim"].current.buffer.options["expandtab"],
        }
        start_line = state["nvim"].eval("v:lnum") - 1
        end_line = start_line + state["nvim"].eval("v:count")
        end_char = len(state["nvim"].current.buffer[end_line]) - 1
        textRange = {
            "start": {"line": start_line, "character": 0},
            "end": {"line": end_line, "character": end_char},
        }

        textEdits = state["rpcs"][languageId].call("textDocument/rangeFormatting", {
            "textDocument": {
                "uri": uri,
            },
            "range": textRange,
            "options": options,
        })

        if textEdits is None or not handle:
            return textEdits

        textDocumentEdit = {
            "textDocument": {
                "uri": uri,
            },
            "edits": textEdits,
        }

        apply_TextDocumentEdit(textDocumentEdit)
        set_cursor(uri, line, character)

        logger.info("End textDocument/rangeFormatting")
        return textEdits

    @neovim.function("LanguageClient_call")
    def call_vim(self, args: List) -> Any:
        """
        Expose RPC call() to vimscript.
        """
        languageId, = gather_args(["languageId"])

        return state["rpcs"][languageId].call(args[0], args[1])

    @neovim.function("LanguageClient_notify")
    def notify_vim(self, args: List) -> None:
        """
        Expose RPC notify() to vimscript.
        """
        languageId, = gather_args(["languageId"])

        state["rpcs"][languageId].notify(args[0], args[1])

    def telemetry_event(self, params: Dict) -> None:
        if params.get("type") == "log":
            echomsg(params.get("message"))

    def window_logMessage(self, params: Dict) -> None:
        msgType = params["type"]
        if msgType > state["windowLogMessageLevel"].value:
            return
        msg = "[{}] {}".format(MessageType(msgType).name, params["message"])  # noqa: F841
        echomsg(msg)

    # Extension by JDT language server.
    def language_status(self, params: Dict) -> None:
        msg = "{} {}".format(params["type"], params["message"])
        echomsg(msg)

    def rustDocument_beginBuild(self, params: Dict) -> None:
        msg = "rustDocument/beginBuild"
        logger.info(msg)

    def rustDocument_diagnosticsBegin(self, params: Dict) -> None:
        msg = "rustDocument/diagnosticsBegin"  # noqa: F841
        logger.info(msg)

    def rustDocument_diagnosticsEnd(self, params: Dict) -> None:
        msg = "rustDocument/diagnosticsEnd"  # noqa: F841
        logger.info(msg)

    def workspace_applyEdit(self, params: Dict) -> None:
        apply_WorkspaceEdit(params["edit"])
        # TODO: write response to server.

    def handle_request_and_notify(self, message: Dict) -> None:
        method = message["method"].replace("/", "_")
        if hasattr(self, method):
            try:
                state["nvim"].async_call(getattr(self, method), message.get("params"))
            except Exception:
                logger.exception("Exception in handle request and notify.")
        else:
            logger.warn("no handler implemented for " + method)
