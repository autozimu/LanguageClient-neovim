import logging

logger = logging.getLogger("LanguageClient")
fileHandler = logging.FileHandler(filename="/tmp/LanguageClient.log")
fileHandler.setFormatter(
    logging.Formatter(
        "%(asctime)s %(levelname)-8s %(message)s",
        "%H:%M:%S"))
logger.addHandler(fileHandler)
logger.setLevel(logging.WARN)
