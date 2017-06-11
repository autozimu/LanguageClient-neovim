import logging
import tempfile

logger = logging.getLogger("LanguageClient")
with tempfile.NamedTemporaryFile(
        prefix="LanguageClient-",
        suffix=".log", delete=False) as tmp:
    tmpname = tmp.name
fileHandler = logging.FileHandler(filename=tmpname)
fileHandler.setFormatter(
    logging.Formatter(
        "%(asctime)s %(levelname)-8s %(message)s",
        "%H:%M:%S"))
logger.addHandler(fileHandler)
logger.setLevel(logging.WARN)
