import json

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

    def call(self, method: str, params: dict, cb):
        if cb is not None: # a call
            mid = self.incMid()
            self.queue[mid] = cb

        content = {
                "jsonrpc": "2.0",
                "method": method,
                "params": params,
                }
        if cb is not None:
            content["id"] = mid

        content = json.dumps(content)
        message = (
                "Content-Length: {}\r\n\r\n"
                "{}".format(len(content), content)
                )
        logger.info('=> ' +  content)
        self.outfile.write(message)
        self.outfile.flush()

    def notify(self, method, params):
        self.call(method, params, None)

    def serve(self):
        while True:
            line = self.infile.readline()
            if line:
                contentLength = int(line.split(":")[1])
                self.infile.readline()
                content = self.infile.read(contentLength)
                logger.info('<= ' + content)
                self.handle(json.loads(content))

    def handle(self, message: dict):
        if "error" in message: # error
            if "id" in message:
                mid = message["id"]
                del self.queue[mid]
            error = message["error"]
            try:
                self.onError(error)
            except:
                logger.exception("Exception in RPC.onError.")
        else "result" in message: # result
            mid = message['id']
            try:
                self.queue[mid](message['result'])
            except:
                logger.exception("Exception in RPC request callback.")
            del self.queue[mid]
        else "method" in message: # request/notification
            method = message["method"]
            params = message["params"]
            if "id" in message: # request
                try:
                    self.onRequest(method, params)
                except:
                    logger.exception("Exception in RPC.onRequest")
            else:
                try:
                    self.onNotification(method, params)
                except:
                    logger.exception("Exception in RPC.onNotification")

