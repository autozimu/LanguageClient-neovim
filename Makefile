install:
	pip3 install neovim --upgrade

test:
	# NVIM_LISTEN_ADDRESS="/tmp/nvim-LanguageClient-IntegrationTest" nvim -N -u tests/vimrc
	py.test
