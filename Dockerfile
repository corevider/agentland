FROM rust:1-bookworm AS build

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps/desktop/src-tauri/Cargo.toml ./apps/desktop/src-tauri/Cargo.toml

RUN mkdir -p apps/desktop/src-tauri/src \
    && echo 'fn main() {}' > apps/desktop/src-tauri/src/main.rs \
    && echo 'fn main() {}' > apps/desktop/src-tauri/build.rs \
    && cargo build --release -p agentland-core --bin agentland-core

FROM node:20-bookworm-slim AS ui

WORKDIR /ui
COPY apps/desktop/package.json apps/desktop/package-lock.json ./
RUN npm ci --no-audit --no-fund

COPY apps/desktop/ ./
RUN npm run build

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        git \
        openssh-client \
        tmux \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --uid 1000 agentland

COPY --from=build /src/target/release/agentland-core /usr/local/bin/agentland-core
COPY --from=ui /ui/dist /opt/agentland/ui

RUN mkdir -p /data /projects && chown -R agentland:agentland /data /projects /opt/agentland

USER agentland
WORKDIR /data

ENV AGENTLAND_PORT=9470

EXPOSE 9470

CMD ["agentland-core"]
