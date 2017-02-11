all: test

install:
	pip3 install neovim mypy flake8 --upgrade

lint:
	mypy --ignore-missing-imports .;  flake8 .

test:
	bash test.sh
