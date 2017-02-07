import json
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

    def incMid(self) -> int:
        mid = self.mid
        self.mid += 1
        return mid

    def call(self, method: str, params: Dict[str, Any], cb) -> None:
        if cb is not None:  # a call
            mid = self.incMid()
            self.queue[mid] = cb

        contentDict = {
                "jsonrpc": "2.0",
                "method": method,
                "params": params,
                }  # type: Dict[str, Any]
        if cb is not None:
            contentDict["id"] = mid

        content = json.dumps(contentDict)
        message = (
                "Content-Length: {}\r\n\r\n"
                "{}".format(len(content), content)
                )
        logger.info('=> ' + content)
        self.outfile.write(message)
        self.outfile.flush()

    def notify(self, method: str, params: Dict[str, Any]) -> None:
        self.call(method, params, None)

    def serve(self):
        while not self.infile.closed:
            line = self.infile.readline()
            if line:
                contentLength = int(line.split(":")[1])
                self.infile.readline()
                content = self.infile.read(contentLength)
                logger.info('<= ' + content)
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
            try:
                self.queue[mid](message['result'])
            except:
                logger.exception("Exception in RPC request callback.")
            del self.queue[mid]
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
