install:
	pip3 install neovim --upgrade

test:
	# NVIM_LISTEN_ADDRESS="/tmp/nvim-LanguageClient-IntegrationTest" nvim -N -u tests/vimrc --headless & echo "$$!" > tests/nvimPID
	# sleep 2s
	py.test --capture=no --exitfirst
	# kill $$(cat tests/nvimPID)
