# Code Toolbox (PLAN-cross-platform.md): the Tauri app in app/ on the toolbox-core crate in core/, plus the
# dev-layout VS Code extension in vscode-dev-layout/. The original Swift app was retired on 2026-10-19.
CARGO_PATH := PATH="$(HOME)/.cargo/bin:$$PATH"
TAURI      := $(CARGO_PATH) npx tauri
APP        := target/release/bundle/macos/Code Toolbox.app
INSTALL_TO := $(HOME)/Applications
VSIX       := build/dev-layout.vsix
CODE       := /Applications/Visual Studio Code.app/Contents/Resources/app/bin/code
PROFILE    := PHP

# A local self-signed "Code Toolbox Dev" certificate keeps the app's identity the same across rebuilds,
# so macOS doesn't ask for Documents access after every build. Without it, the build stays ad-hoc signed.
SIGN_IDENTITY ?= $(shell security find-identity -p codesigning 2>/dev/null | grep -q '"Code Toolbox Dev"' && echo "Code Toolbox Dev")

.PHONY: build test install run dev screenshots vsix install-ext publish-mac clean

app/node_modules: app/package-lock.json
	cd app && npm ci
	touch app/node_modules

build: app/node_modules
	cd app && $(TAURI) build
	@if [ -n "$(SIGN_IDENTITY)" ]; then codesign --force --sign "$(SIGN_IDENTITY)" "$(APP)"; fi

test: app/node_modules
	$(CARGO_PATH) cargo test -p toolbox-core
	cd app && npx tsc --noEmit

# Replaces the installed copy and starts it. Waits for the old copy to exit: while it's still there, the
# new one would hand over to it (single instance) and quit.
install: build
	pkill -x code-toolbox || true
	while pgrep -x code-toolbox >/dev/null; do sleep 0.2; done
	mkdir -p "$(INSTALL_TO)"
	rm -rf "$(INSTALL_TO)/Code Toolbox.app"
	cp -R "$(APP)" "$(INSTALL_TO)/"
	open "$(INSTALL_TO)/Code Toolbox.app"

run: install

# The panel in a plain browser on fixture data (app/src/mock.ts): http://localhost:1420
dev: app/node_modules
	cd app && npm run dev

# README pictures (docs/images) from the fixture data, in headless Chrome.
screenshots: app/node_modules
	cd app && npx vite --port 1420 --strictPort & pid=$$!; \
	until curl -s -o /dev/null http://localhost:1420/; do sleep 0.5; done; \
	node scripts/screenshots.mjs docs/images; status=$$?; kill $$pid; exit $$status

# The app bundles it too (tauri.conf.json runs this before every build) and installs it from Settings.
vsix: app/node_modules
	cd app && npm run vsix

install-ext: vsix
	"$(CODE)" --profile "$(PROFILE)" --install-extension "$(VSIX)" --force

# The macOS .dmg for a release; the Linux and Windows builds come from GitLab CI (.gitlab-ci.yml).
publish-mac: app/node_modules
	$(CARGO_PATH) scripts/publish-mac.sh

clean:
	rm -rf build target app/dist
