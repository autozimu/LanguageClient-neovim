all: lint test

install:
	pip3 install neovim mypy flake8 --upgrade

test:
	bash test.sh

lint:
	mypy .
	flake8 .
