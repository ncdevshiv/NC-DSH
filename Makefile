# Short names for the Web and Desktop application commands. The package.json
# scripts they call stay the source of truth; docs/development.md documents both.
.DEFAULT_GOAL := help
.PHONY: help build clean web desktop dev-web dev-desktop

BUN ?= bun
ARGS ?=

help:
	@echo "make build        bun run build           complete repository build"
	@echo "make clean        bun run clean           remove build outputs and safe deleted-package residue"
	@echo "make web          bun run start:web       serve the built Web artifacts from source"
	@echo "make desktop      bun run start:desktop   launch the built Desktop artifacts"
	@echo "make dev-web      bun run dev:web         build, serve, and rebuild Web on source edits"
	@echo "make dev-desktop  bun run dev:desktop     build, then launch Desktop"
	@echo "ARGS='--no-open --port 3081' forwards options to the launched application;"
	@echo "the Web commands accept dsh web flags, the Desktop launcher accepts none."

build:
	$(BUN) run build

clean:
	$(BUN) run clean

web:
	$(BUN) run start:web $(ARGS)

desktop:
	$(BUN) run start:desktop $(ARGS)

dev-web:
	$(BUN) run dev:web $(ARGS)

dev-desktop:
	$(BUN) run dev:desktop $(ARGS)
