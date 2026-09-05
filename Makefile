PREFIX ?= /usr/local

.PHONY: install uninstall

install:
	cargo install --path . --root $(PREFIX) --force

uninstall:
	rm -f $(PREFIX)/bin/pen
