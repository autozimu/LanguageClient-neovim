import logging

logger = logging.getLogger('LanguageClient')
fileHandler = logging.FileHandler(filename='/tmp/LanguageClient.log')
fileHandler.setFormatter(
        logging.Formatter(
            '%(asctime)s %(levelname)-8s (%(name)s) %(message)s'))
logger.addHandler(fileHandler)
logger.setLevel(logging.ERROR)
