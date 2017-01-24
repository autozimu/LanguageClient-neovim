install:
	pip3 install neovim --upgrade

test:
	py.test --version
	py.test
