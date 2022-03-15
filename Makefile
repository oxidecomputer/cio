init:
	git config core.hooksPath .githooks

install-bitbar:
	brew cask install bitbar
	mkdir -p ~/.bitbar
	defaults write com.matryer.BitBar pluginsDirectory "~/.bitbar"
	ln rfds.1s.sh ~/.bitbar # hardlink, hopefully on the same filesystem
	open /Applications/BitBar.app
