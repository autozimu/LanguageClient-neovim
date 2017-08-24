all: test lint

install:
	pip3 install neovim mypy flake8 --upgrade

test:
	bash test.sh

lint:
	mypy .
	flake8 .
