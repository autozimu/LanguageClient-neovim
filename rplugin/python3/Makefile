all: lint test

install:
	pip3 install neovim mypy flake8 --upgrade

test:
	bash test.sh

lint:
	mypy .
	-mypy denite/source deoplete/sources
	flake8 . --ignore=E501
