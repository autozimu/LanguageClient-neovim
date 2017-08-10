import logging
import os

logger = logging.getLogger("LanguageClient")
filename = os.getenv('TMP', '/tmp') + "/LanguageClient.log"
fileHandler = logging.FileHandler(filename)
fileHandler.setFormatter(
    logging.Formatter(
        "%(asctime)s %(levelname)-8s %(message)s",
        "%H:%M:%S"))
logger.addHandler(fileHandler)
logger.setLevel(logging.WARN)
