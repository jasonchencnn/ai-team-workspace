FROM ubuntu:24.04@sha256:008173c23f95b170204355c12626cb5a965d779a7e1283b09e9cffbb1bf33ca3

ARG TARGETARCH
ARG NODE_VERSION=24.21.0
ARG PNPM_VERSION=10.33.0
ARG RUST_VERSION=1.98.0
ARG GITLEAKS_VERSION=8.30.0
ARG CARGO_AUDIT_VERSION=0.22.0
ARG CARGO_DENY_VERSION=0.18.6
ARG NODE_LINUX_AMD64_SHA256=fd8e59d5a511510f6a298afb548f18c7d2b1be404d8b4a27d94fbe49f56cb2d6
ARG NODE_LINUX_ARM64_SHA256=6ad1325edbdb5649c379b75a237147a666c95d4f9ae8d340fef2d1575d289ad2
ARG GITLEAKS_LINUX_AMD64_SHA256=79a3ab579b53f71efd634f3aaf7e04a0fa0cf206b7ed434638d1547a2470a66e
ARG GITLEAKS_LINUX_ARM64_SHA256=b4cbbb6ddf7d1b2a603088cd03a4e3f7ce48ee7fd449b51f7de6ee2906f5fa2f
ARG PNPM_INTEGRITY=sha512-EFaLtKavtYyes2MNqQzJUWQXq+vT+rvmc58K55VyjaFJHp21pUTHatjrdXD1xLs9bGN7LLQb/c20f6gjyGSTGQ==
ARG RUSTUP_INIT_SHA256=7d0ea0f8eba7fa1ebfe998091cd7ec4501e33ec5ca6b884eb4d894d7da5170af

ENV DEBIAN_FRONTEND=noninteractive
ENV CARGO_HOME=/opt/cargo
ENV CARGO_BUILD_JOBS=2
ENV RUSTUP_HOME=/opt/rustup
ENV PATH=/opt/cargo/bin:/opt/node/bin:${PATH}

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
      build-essential \
      ca-certificates \
      curl \
      file \
      git \
      libayatana-appindicator3-dev \
      librsvg2-dev \
      libwebkit2gtk-4.1-dev \
      libxdo-dev \
      patchelf \
      pkg-config \
      python3 \
      xz-utils \
    && rm -rf /var/lib/apt/lists/*

RUN case "${TARGETARCH}" in \
      amd64) node_arch="x64"; node_sha="${NODE_LINUX_AMD64_SHA256}"; gitleaks_arch="x64"; gitleaks_sha="${GITLEAKS_LINUX_AMD64_SHA256}" ;; \
      arm64) node_arch="arm64"; node_sha="${NODE_LINUX_ARM64_SHA256}"; gitleaks_arch="arm64"; gitleaks_sha="${GITLEAKS_LINUX_ARM64_SHA256}" ;; \
      *) echo "unsupported architecture: ${TARGETARCH}" >&2; exit 1 ;; \
    esac \
    && curl --fail --location --show-error --silent \
      "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${node_arch}.tar.xz" \
      --output /tmp/node.tar.xz \
    && echo "${node_sha}  /tmp/node.tar.xz" | sha256sum --check --strict \
    && mkdir -p /opt/node \
    && tar --extract --xz --file /tmp/node.tar.xz --strip-components=1 --directory /opt/node \
    && rm /tmp/node.tar.xz \
    && test "$(npm view "pnpm@${PNPM_VERSION}" dist.integrity --registry=https://registry.npmjs.org)" = "${PNPM_INTEGRITY}" \
    && npm install --global "pnpm@${PNPM_VERSION}" --registry=https://registry.npmjs.org \
    && curl --fail --location --show-error --silent \
      "https://github.com/gitleaks/gitleaks/releases/download/v${GITLEAKS_VERSION}/gitleaks_${GITLEAKS_VERSION}_linux_${gitleaks_arch}.tar.gz" \
      --output /tmp/gitleaks.tar.gz \
    && echo "${gitleaks_sha}  /tmp/gitleaks.tar.gz" | sha256sum --check --strict \
    && tar --extract --gzip --file /tmp/gitleaks.tar.gz --directory /usr/local/bin gitleaks \
    && rm /tmp/gitleaks.tar.gz

RUN curl --proto '=https' --tlsv1.2 --fail --show-error --silent \
      https://sh.rustup.rs --output /tmp/rustup-init.sh \
    && echo "${RUSTUP_INIT_SHA256}  /tmp/rustup-init.sh" | sha256sum --check --strict \
    && sh /tmp/rustup-init.sh \
      --default-toolchain "${RUST_VERSION}" \
      --profile minimal \
      --component clippy \
      --component rustfmt \
      --no-modify-path \
      --yes \
    && rm /tmp/rustup-init.sh \
    && cargo install cargo-audit --version "${CARGO_AUDIT_VERSION}" --locked \
    && cargo install cargo-deny --version "${CARGO_DENY_VERSION}" --locked \
    && chmod -R a+rX /opt/cargo /opt/rustup /opt/node

RUN test "$(id -u ubuntu)" = "1000" \
    && mkdir -p /workspace /evidence \
    && chown ubuntu:ubuntu /workspace /evidence

USER ubuntu
WORKDIR /workspace

CMD ["node", "scripts/ci/local-gates.mjs", "--platform", "ubuntu", "--evidence", "/evidence/DEV-01-05-ubuntu-arm64.json"]
