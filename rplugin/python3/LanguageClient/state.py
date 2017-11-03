import greenlet
import json
from typing import Dict, Any, List

from .DiagnosticsDisplay import DiagnosticsDisplay
from .logger import logger, logpath_server
from .util import escape
from .MessageType import MessageType

state = {
    "nvim": None,
    "servers": {},  # Dict[str, subprocess.Popen]. language id to subprocess.
    "rpcs": {},  # Dict[str, RPC]. language id to RPC instance.
    "handlers": {},  # Dict[int, PyGreenlet]. message id to greenlet.
    "capabilities": {},  # Dict[str, Dict]. language id to capabilities.
    "rootUris": {},  # Dict[str, str]. language id to rootUri.

    "last_cursor_line": -1,
    "last_line_diagnostic": "",
    "codeActionCommands": [],  # List[Command]. Stashed codeAction commands.

    # Settings
    "serverCommands": {},  # Dict[str, List[str]]. language id to server command.
    "autoStart": False,  # Whether auto start servers.
    "changeThreshold": 0,  # change notification threshold.
    "selectionUI": None,  # UI for select an item.
    "trace": "off",  # option passed onto server.
    "diagnosticsEnable": True,  # whether to show diagnostics.
    "diagnosticsList": "quickfix",  # location to store error list.
    "diagnosticsDisplay": DiagnosticsDisplay,  # how to display diagnostics.
    # Maximum MessageType to echoshow messages from window/logMessage.
    "windowLogMessageLevel": MessageType.Warning,
}  # type: Dict[str, Any]


def update_state(u):
    """
    Merge state with partial state.
    """
    global state
    state = _update_state_helper(state, u, "state")


def _update_state_helper(d: Dict, u: Dict, path: str) -> Dict:
    """
    Merge dicts.
    """
    for key, value in u.items():
        next_path = path + "." + str(key)
        if isinstance(value, dict):
            d[key] = _update_state_helper(d.get(key, {}), value, next_path)
        else:
            if d.get(key) != value:
                logger.debug("{}: {} => {}".format(next_path, d.get(key), value))
            d[key] = value
    return d


def set_state(path: List[str], v: Any) -> None:
    """
    Set part of state.
    """
    global state
    node = state
    for (i, key) in enumerate(path):
        if i < len(path) - 1:
            if key not in node:
                node[key] = {}
            node = node[key]
        else:
            logger.debug("state.{}: {} => {}".format(
                str.join(".", path), node.get(key), v))
            node[key] = v


def make_serializable(d: Any) -> Any:
    """
    Clone an object. Skip parts not serializable.
    """
    if isinstance(d, dict):
        d2 = {}
        for k, v in d.items():
            v = make_serializable(v)
            if v is None:
                continue
            d2[k] = v
        return d2
    elif isinstance(d, list):
        l = d
        l2 = []
        for i in l:
            i2 = make_serializable(i)
            if i2 is None:
                continue
            l2.append(i2)
        return l2
    else:
        try:
            json.dumps(d)
            return d
        except Exception:
            return None


def suspend(mid: int) -> Dict:
    gr = greenlet.getcurrent()
    state["handlers"][mid] = gr
    response = gr.parent.switch()

    if handle_error(response):
        return None
    else:
        return response["result"]


def wake_up(mid: int, result: Any) -> None:
    handler = state["handlers"][mid]
    del state["handlers"][mid]
    handler.parent = greenlet.getcurrent()
    handler.switch(result)


def handle_error(response: Dict) -> bool:
    if "error" in response:
        logger.error(str(response))
        echoerr(json.dumps(response))
        return True
    else:
        return False


def execute_command(command: str) -> None:
    """Execute vim command."""
    state["nvim"].command(command)


def echo(message: str) -> None:
    """Echo message."""
    message = escape(message)
    execute_command("echo '{}'".format(message))


def echomsg(message: str) -> None:
    """Echo message"""
    message = escape(message)
    execute_command("echomsg '{}'".format(message))


def echoerr(message: str) -> None:
    """Echo message as error."""
    message = escape(message)
    execute_command("echohl Error | echomsg '{}' | echohl None".format(message))


def echo_ellipsis(msg: str, columns: int) -> None:
    """
    Print as much of msg as possible without triggering "Press Enter"
    prompt.

    Inspired by neomake, which is in turn inspired by syntastic.
    """
    msg = msg.replace("\n", ". ")
    if len(msg) > columns - 12:
        msg = msg[:columns - 15] + "..."

    echo(msg)


def alive(languageId: str, warn: bool) -> bool:
    """Check if language server for language id is alive."""
    msg = None
    if state["servers"].get(languageId) is None:
        msg = "Language client is not running. Try :LanguageClientStart"
    elif state["servers"][languageId].poll() is not None:
        msg = "Failed to start language server. See {}.".format(logpath_server)
    if msg and warn:
        logger.warn(msg)
        echoerr(msg)
    return msg is None
