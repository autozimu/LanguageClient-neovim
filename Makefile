install:
	pip3 install neovim --upgrade

test:
	nvim +UpdateRemotePlugins +qall
	rm -f /tmp/nvim-LanguageClient-IntegrationTest
	NVIM_LISTEN_ADDRESS=/tmp/nvim-LanguageClient-IntegrationTest nvim -n -u tests/vimrc --headless & echo "$$!" > tests/nvimPID
	sleep 1s && py.test --capture=no --exitfirst
	kill $$(cat tests/nvimPID)
