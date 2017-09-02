import greenlet
import json
from typing import Dict, Any

from .DiagnosticsDisplay import DiagnosticsDisplay
from .logger import logger
from .util import escape

state = {
    "nvim": None,
    "servers": {},  # Dict[str, subprocess.Popen]. language id to subprocess.
    "rpcs": {},  # Dict[str, RPC]. language id to RPC instance.
    "handlers": {},  # Dict[int, PyGreenlet]. message id to greenlet.
    "capabilities": {},  # Dict[str, Dict]. language id to capabilities.
    "rootUris": {},  # Dict[str, str]. language id to rootUri.
    "textDocuments": {},  # Dict[str, TextDocumentItem]. uri to TextDocumentItem.
    "line_diagnostics": {},  # Dict[str, Dict[int, Dict]]. uri to line number to diagnostic message.
    "last_cursor_line": -1,
    "highlight_source_id": None,
    "signs": [],  # diagnostic signs

    # Settings
    "serverCommands": {},  # Dict[str, List[str]]. language id to server command.
    "autoStart": False,  # Whether auto start servers.
    "changeThreshold": 0,  # change notification threshold.
    "selectionUI": None,  # UI for select an item.
    "trace": "off",  # option passed onto server.
    "diagnosticsEnable": True,  # whether to show diagnostics.
    "diagnosticsList": "quickfix",  # location to store error list.
    "diagnosticsDisplay": DiagnosticsDisplay,  # how to display diagnostics.
}  # type: Dict[str, Any]


def update_state(u):
    global state
    state = _update_helper(state, u, "state")


def _update_helper(d: Dict, u: Dict, path: str) -> Dict:
    for key, value in u.items():
        next_path = path + "." + str(key)
        if isinstance(value, dict):
            d[key] = _update_helper(d.get(key, {}), value, next_path)
        else:
            if d.get(key) != value:
                logger.debug("{}: {} -> {}".format(next_path, d.get(key), value))
            d[key] = value
    return d


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
        echomsg(json.dumps(response))
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
    """Echomsg message"""
    message = escape(message)
    execute_command("echomsg '{}'".format(message))


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
