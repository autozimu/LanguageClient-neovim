from collections import OrderedDict
import json
import time
from typing import Dict, Any

from . logger import logger

from .state import state, suspend, wake_up, alive


class RPC:
    def __init__(self, infile, outfile, on_call, languageId: str) -> None:
        self.infile = infile
        self.outfile = outfile
        self.on_call = on_call
        self.languageId = languageId
        self.mid = 0

    def inc_mid(self) -> int:
        mid = self.mid
        self.mid += 1
        return mid

    def send_message(self, payload_dict: Dict[str, Any]) -> None:
        payload = json.dumps(payload_dict, separators=(',', ':'))
        message = (
            "Content-Length: {}\r\n\r\n"
            "{}".format(len(payload.encode("UTF-8")), payload)
        )
        logger.debug("=> " + payload)
        self.outfile.write(message.encode("UTF-8"))
        self.outfile.flush()

    def call(self, method: str, params: Dict[str, Any]) -> Dict:
        """
        """
        mid = self.inc_mid()

        message = OrderedDict([
            ("jsonrpc", "2.0"),
            ("id",      mid),
            ("method",  method),
            ("params",  params)
        ])  # type: Dict[str, Any]

        self.send_message(message)

        return suspend(mid)

    def notify(self, method: str, params: Dict[str, Any]) -> None:

        message = OrderedDict([
            ("jsonrpc", "2.0"),
            ("method",  method),
            ("params",  params)
        ])  # type: Dict[str, Any]

        self.send_message(message)

    def serve(self):
        content_length = 0
        while not self.infile.closed:
            try:
                data = self.infile.readline()
                line = data.decode("UTF-8").strip()
            except UnicodeError:
                msg = "Failed to decode message as UTF-8: " + str(data)
                logger.exception(msg)
                continue
            if line:
                header, value = line.split(":")
                if header == "Content-Length":
                    content_length = int(value)
            else:
                try:
                    data = self.infile.read(content_length)
                    content = data.decode("UTF-8")
                except UnicodeError:
                    msg = "Failed to decode message as UTF-8: " + str(data)
                    logger.exception(msg)
                    continue
                logger.debug("<= " + content)
                try:
                    msg = json.loads(content)
                except Exception:
                    msg = "Error deserializing server output: " + content
                    logger.exception(msg)
                    isAlive = alive(self.languageId, warn=False)
                    if isAlive:
                        time.sleep(1.0)
                        continue
                    else:
                        msg = "Server process exited. Stopping RPC thread."
                        logger.info(msg)
                        break
                try:
                    self.handle(msg)
                except Exception:
                    msg = "Error handling message: " + content
                    logger.exception(msg)

    def handle(self, message: Dict[str, Any]) -> None:
        if "result" in message or "error" in message:  # response
            mid = message["id"]
            if isinstance(mid, str):
                mid = int(mid)

            state["nvim"].async_call(wake_up, mid, message)
        elif "method" in message:  # request/notify
            self.on_call(message)
        else:
            logger.error("Unknown message.")
