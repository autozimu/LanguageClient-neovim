import json
from threading import Condition
from typing import Dict, Any

from . logger import logger


class RPC:
    def __init__(self, infile, outfile, onRequest, onNotification):
        self.infile = infile
        self.outfile = outfile
        self.onRequest = onRequest
        self.onNotification = onNotification
        self.mid = 0
        self.queue = {}
        self.cv = Condition()
        self.result = None

    def incMid(self) -> int:
        mid = self.mid
        self.mid += 1
        return mid

    def message(self, contentDict: Dict[str, Any]) -> None:
        content = json.dumps(contentDict, separators=(',', ':'))
        message = (
            "Content-Length: {}\r\n\r\n"
            "{}".format(len(content.encode('utf-8')), content)
        )
        logger.debug(' => ' + content)
        self.outfile.write(message.encode('utf-8'))
        self.outfile.flush()

    def call(self, method: str, params: Dict[str, Any], cbs=None):
        """
        @param cbs: func list. Callbacks to handle result or error. If None,
            turn to sync call.
        """
        mid = self.incMid()
        if cbs is not None:
            self.queue[mid] = cbs

        contentDict = {
            "jsonrpc": "2.0",
            "id": mid,
            "method": method,
            "params": params,
        }  # type: Dict[str, Any]
        self.message(contentDict)

        if cbs is not None:
            return

        with self.cv:
            if not self.cv.wait_for(lambda: self.result is not None, 3):
                return None
            result = self.result
            self.result = None
            return result

    def notify(self, method: str, params: Dict[str, Any]) -> None:
        contentDict = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }  # type: Dict[str, Any]
        self.message(contentDict)

    def serve(self):
        self.run = True
        contentLength = 0
        while not self.infile.closed:
            line = self.infile.readline().decode('utf-8').strip()
            if line:
                header, value = line.split(":")
                if header == "Content-Length":
                    contentLength = int(value)
            else:
                content = self.infile.read(contentLength).decode('utf-8')
                logger.debug(' <= ' + content)
                try:
                    msg = json.loads(content)
                except Exception:
                    if not self.run:
                        break
                    msg = "Error deserializing server output: " + content
                    self.onError(msg)
                    logger.exception(msg)
                    continue
                try:
                    self.handle(msg)
                except Exception:
                    msg = "Error handling message: " + content
                    self.onError(msg)
                    logger.exception(msg)

    def handle(self, message: Dict[str, Any]):
        if "result" in message or "error" in message:
            mid = message["id"]
            if isinstance(mid, str):
                mid = int(mid)
            if mid in self.queue:  # async call
                cbs = self.queue[mid]
                del self.queue[mid]
                if "result" in message:
                    cbs[0](message["result"])
                else:
                    logger.error(json.dumps(message))
                    cbs[1](message["error"])
            else:  # sync call
                with self.cv:
                    if "result" in message:
                        self.result = message["result"]
                    else:
                        logger.error(json.dumps(message))
                        self.result = []
                    self.cv.notify()
        elif "method" in message:  # request/notification from server
            if "id" in message:
                self.onRequest(message)
            else:
                self.onNotification(message)
        else:
            logger.error('Unknown message.')
