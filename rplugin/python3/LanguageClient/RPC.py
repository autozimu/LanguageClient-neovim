import json
from threading import Condition
from typing import Dict, Any

from . logger import logger


class RPC:
    def __init__(self, infile, outfile, onRequest, onNotification, onError):
        self.infile = infile
        self.outfile = outfile
        self.onRequest = onRequest
        self.onNotification = onNotification
        self.onError = onError
        self.mid = 0
        self.queue = {}
        self.cv = Condition()
        self.result = None

    def incMid(self) -> int:
        mid = self.mid
        self.mid += 1
        return mid

    def message(self, contentDict: Dict[str, Any]) -> None:
        content = json.dumps(contentDict)
        message = (
                "Content-Length: {}\r\n\r\n"
                "{}".format(len(content), content)
                )
        logger.debug(' => ' + content)
        self.outfile.write(message)
        self.outfile.flush()

    def call(self, method: str, params: Dict[str, Any], cb=None):
        """
        @param cb: func. Callback to handle result. If None, turn to sync call.
        """
        mid = self.incMid()
        if cb is not None:
            self.queue[mid] = cb

        contentDict = {
                "jsonrpc": "2.0",
                "method": method,
                "params": params,
                "id": mid,
                }  # type: Dict[str, Any]
        self.message(contentDict)

        if cb is not None:
            return

        with self.cv:
            while self.result is None:
                self.cv.wait()
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
        while not self.infile.closed:
            line = self.infile.readline()
            if line:
                contentLength = int(line.split(":")[1])
                self.infile.readline()
                content = self.infile.read(contentLength)
                logger.debug(' <= ' + content)
                self.handle(json.loads(content))

    def handle(self, message: Dict[str, Any]):
        if "error" in message:  # error
            if "id" in message:
                mid = message["id"]
                del self.queue[mid]
            try:
                self.onError(message)
            except:
                logger.exception("Exception in RPC.onError.")
        elif "result" in message:  # result
            mid = message['id']
            result = message["result"]
            if mid in self.queue:  # async call
                try:
                    self.queue[mid](result)
                except:
                    logger.exception("Exception in RPC request callback.")
                del self.queue[mid]
            else:  # sync call
                with self.cv:
                    self.result = result
                    self.cv.notify()
        elif "method" in message:  # request/notification
            if "id" in message:  # request
                try:
                    self.onRequest(message)
                except:
                    logger.exception("Exception in RPC.onRequest")
            else:
                try:
                    self.onNotification(message)
                except:
                    logger.exception("Exception in RPC.onNotification")
        else:
            logger.error('Unexpected')
