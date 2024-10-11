# Heavily inspired by Lighthouse: https://github.com/sigp/lighthouse/blob/693886b94176faa4cb450f024696cb69cda2fe58/Makefile
#
# and Reth: https://github.com/paradigmxyz/reth/blob/2e87b2a8d57813ce61f8898cf89d7b0dda2ab27d/Makefile
.DEFAULT_GOAL := help

GIT_TAG ?= $(shell git describe --tags --abbrev=0)
BIN_DIR = "dist/bin"

# Cargo profile for builds. Default is for local builds, CI uses an override.
PROFILE ?= release

# The docker image name
DOCKER_IMAGE_NAME ?= ghcr.io/ithacaxyz/odyssey

BUILD_PATH = "target"

SOLIDITY_BUILD_IMAGE = "ghcr.io/paradigmxyz/foundry-alphanet@sha256:dec045ad42b69952cc02800bc8a749caaa899dbae5a73e31674d19cdb057dc14"

# List of features to use when building. Can be overridden via the environment.
# No jemalloc on Windows
ifeq ($(OS),Windows_NT)
    FEATURES ?=
else
    FEATURES ?= jemalloc asm-keccak
endif

##@ Help

.PHONY: help
help: ## Display this help.
	@awk 'BEGIN {FS = ":.*##"; printf "Usage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_0-9-]+:.*?##/ { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

.PHONY: build-eip3074-bytecode
build-eip3074-bytecode:
	docker run --rm \
		-v $$(pwd)/crates/testing/resources/eip3074:/app/foundry \
		-u $$(id -u):$$(id -g) \
		$(SOLIDITY_BUILD_IMAGE) \
		--foundry-directory /app/foundry \
		--foundry-command "forge build"

.PHONY: check-eip3074-bytecode
check-eip3074-bytecode: build-eip3074-bytecode
	@if ! git diff --exit-code --quiet; then \
		echo "Error: There are uncommitted changes after the build. Please run 'make build-eip3074-bytecode' and commit the changes"; \
		exit 1; \
	fi

.PHONY: build-bls12-381-bytecode
build-bls12-381-bytecode:
	docker run --rm \
		-v $$(pwd)/crates/testing/resources/bls12-381:/app/foundry \
		-u $$(id -u):$$(id -g) \
		$(SOLIDITY_BUILD_IMAGE) \
		--foundry-directory /app/foundry \
		--foundry-command "forge build"

.PHONY: check-bls12-381-bytecode
check-bls12-381-bytecode: build-bls12-381-bytecode
	@if ! git diff --exit-code --quiet; then \
		echo "Error: There are uncommitted changes after the build. Please run 'make build-bls12-381-bytecode' and commit the changes"; \
		exit 1; \
	fi

.PHONY: check-bytecode
check-bytecode: check-eip3074-bytecode check-bls12-381-bytecode

# The following commands use `cross` to build a cross-compile.
#
# These commands require that:
#
# - `cross` is installed (`cargo install cross`).
# - Docker is running.
# - The current user is in the `docker` group.
#
# The resulting binaries will be created in the `target/` directory.

# For aarch64, disable asm-keccak optimizations and set the page size for
# jemalloc. When cross compiling, we must compile jemalloc with a large page
# size, otherwise it will use the current system's page size which may not work
# on other systems. JEMALLOC_SYS_WITH_LG_PAGE=16 tells jemalloc to use 64-KiB
# pages. See: https://github.com/paradigmxyz/reth/issues/6742
build-aarch64-unknown-linux-gnu: FEATURES := $(filter-out asm-keccak,$(FEATURES))
build-aarch64-unknown-linux-gnu: export JEMALLOC_SYS_WITH_LG_PAGE=16

# No jemalloc on Windows
build-x86_64-pc-windows-gnu: FEATURES := $(filter-out jemalloc jemalloc-prof,$(FEATURES))

# Note: The additional rustc compiler flags are for intrinsics needed by MDBX.
# See: https://github.com/cross-rs/cross/wiki/FAQ#undefined-reference-with-build-std
build-%:
	RUSTFLAGS="-C link-arg=-lgcc -Clink-arg=-static-libgcc" \
		cross build --bin odyssey --target $* --features "$(FEATURES)" --profile "$(PROFILE)"

# Unfortunately we can't easily use cross to build for Darwin because of licensing issues.
# If we wanted to, we would need to build a custom Docker image with the SDK available.
#
# Note: You must set `SDKROOT` and `MACOSX_DEPLOYMENT_TARGET`. These can be found using `xcrun`.
#
# `SDKROOT=$(xcrun -sdk macosx --show-sdk-path) MACOSX_DEPLOYMENT_TARGET=$(xcrun -sdk macosx --show-sdk-platform-version)`
build-x86_64-apple-darwin:
	$(MAKE) build-native-x86_64-apple-darwin
build-aarch64-apple-darwin:
	$(MAKE) build-native-aarch64-apple-darwin

##@ Docker

# Note: This requires a buildx builder with emulation support. For example:
#
# `docker run --privileged --rm tonistiigi/binfmt --install amd64,arm64`
# `docker buildx create --use --driver docker-container --name cross-builder`
.PHONY: docker-build-push
docker-build-push: ## Build and push a cross-arch Docker image tagged with the latest git tag.
	$(call docker_build_push,$(GIT_TAG),$(GIT_TAG))

# Note: This requires a buildx builder with emulation support. For example:
#
# `docker run --privileged --rm tonistiigi/binfmt --install amd64,arm64`
# `docker buildx create --use --driver docker-container --name cross-builder`
.PHONY: docker-build-push-latest
docker-build-push-latest: ## Build and push a cross-arch Docker image tagged with the latest git tag and `latest`.
	$(call docker_build_push,$(GIT_TAG),latest)

# Note: This requires a buildx builder with emulation support. For example:
#
# `docker run --privileged --rm tonistiigi/binfmt --install amd64,arm64`
# `docker buildx create --use --name cross-builder`
.PHONY: docker-build-push-nightly
docker-build-push-nightly: ## Build and push cross-arch Docker image tagged with the latest git tag with a `-nightly` suffix, and `latest-nightly`.
	$(call docker_build_push,$(GIT_TAG)-nightly,latest-nightly)

# Create a cross-arch Docker image with the given tags and push it
define docker_build_push
	$(MAKE) build-x86_64-unknown-linux-gnu
	mkdir -p $(BIN_DIR)/amd64
	cp $(BUILD_PATH)/x86_64-unknown-linux-gnu/$(PROFILE)/odyssey $(BIN_DIR)/amd64/odyssey

	$(MAKE) build-aarch64-unknown-linux-gnu
	mkdir -p $(BIN_DIR)/arm64
	cp $(BUILD_PATH)/aarch64-unknown-linux-gnu/$(PROFILE)/odyssey $(BIN_DIR)/arm64/odyssey

	docker buildx build --file ./Dockerfile.cross . \
		--platform linux/amd64,linux/arm64 \
		--tag $(DOCKER_IMAGE_NAME):$(1) \
		--tag $(DOCKER_IMAGE_NAME):$(2) \
		--provenance=false \
		--push
endef
