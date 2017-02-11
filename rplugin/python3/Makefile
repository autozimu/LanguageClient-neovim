all: test

install:
	pip3 install neovim mypy flake8 --upgrade

lint:
	mypy --ignore-missing-imports .;  flake8 .

test:
	nvim +UpdateRemotePlugins +qall
	rm -f /tmp/nvim-LanguageClient-IntegrationTest
	cat /dev/null > /tmp/LanguageClient.log
	NVIM_LISTEN_ADDRESS=/tmp/nvim-LanguageClient-IntegrationTest nvim -n -u tests/vimrc --headless 2>/dev/null & echo "$$!" > tests/nvimPID
	sleep 1s
	rm -rf LanguageClient/__pycache__
	py.test --capture=no --exitfirst; ret=$$?; cat /tmp/LanguageClient.log; kill $$(cat tests/nvimPID); exit $$ret
