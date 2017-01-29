all: test

install:
	pip3 install neovim mypy flake8 --upgrade

lint:
	mypy --ignore-missing-imports .;  flake8 .

test:
	nvim +UpdateRemotePlugins +qall
	rm -f /tmp/nvim-LanguageClient-IntegrationTest
	NVIM_LISTEN_ADDRESS=/tmp/nvim-LanguageClient-IntegrationTest nvim -n -u tests/vimrc --headless 2>/dev/null & echo "$$!" > tests/nvimPID
	sleep 1s && py.test --capture=no --exitfirst
	kill $$(cat tests/nvimPID)
