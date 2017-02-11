all: test

install:
	pip3 install neovim mypy flake8 --upgrade

lint:
	mypy --ignore-missing-imports .;  flake8 .

test:
	nvim +UpdateRemotePlugins +qall
	rm -f /tmp/nvim-LanguageClient-IntegrationTest
	NVIM_LISTEN_ADDRESS=/tmp/nvim-LanguageClient-IntegrationTest nvim -n -u tests/vimrc --headless 2>/dev/null & echo "$$!" > tests/nvimPID
	rm -rf LanguageClient/__pycache__
	sleep 1s
	cat /dev/null > /tmp/LanguageClient.log
	py.test --capture=no --exitfirst
	ret=$$?
	cat /tmp/LanguageClient.log
	-kill $$(cat tests/nvimPID)
	exit $$ret
