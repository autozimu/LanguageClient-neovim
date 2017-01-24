import json

from . logger import logger

class RPC:
    def __init__(self, infile, outfile, handler):
        self.infile = infile
        self.outfile = outfile
        self.handler = handler

    def call(self, method, params, mid=None):
        content = {
                "jsonrpc": "2.0",
                "method": method,
                "params": params
                }
        if mid is not None:
            content["id"] = mid
        content = json.dumps(content)
        message = (
                "Content-Length: {}\r\n\r\n"
                "{}".format(len(content), content)
                )
        logger.info(content)
        self.outfile.write(message)
        self.outfile.flush()

    def serve(self):
        while True:
            line = self.infile.readline()
            if line:
                contentLength = int(line.split(":")[1])
                self.infile.readline()
                content = self.infile.read(contentLength)
                logger.info(content)
                self.handler.handle(json.loads(content))

