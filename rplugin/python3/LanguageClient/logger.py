import logging
import logging.handlers
import os

logger = logging.getLogger("LanguageClient")
logpath = os.path.join(os.getenv("TMP", "/tmp"), "LanguageClient.log")
logpath_server = os.path.join(os.getenv("TMP", "/tmp"), "LanguageServer.log")
fileHandler = logging.handlers.RotatingFileHandler(logpath, maxBytes=1024 ** 20, backupCount=2)
fileHandler.setFormatter(
    logging.Formatter(
        "%(asctime)s %(levelname)-7s [%(threadName)-10s] %(message)s",
        "%H:%M:%S"))
logger.addHandler(fileHandler)
logger.setLevel(logging.WARN)


def setLoggingLevel(level) -> None:
    """
    Set logging level.
    """
    logger.setLevel({
        "ERROR": 40,
        "WARNING": 30,
        "INFO": 20,
        "DEBUG": 10,
    }[level])
