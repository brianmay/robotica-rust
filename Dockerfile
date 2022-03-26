FROM rust:1.59-bullseye as builder
WORKDIR /brian-node-rust

RUN apt-get update \
    && apt-get install -y cmake \
    && rm -rf /var/lib/apt/lists/*

COPY ./Cargo.toml Cargo.lock ./
COPY ./brian-node-rust/Cargo.toml ./brian-node-rust/Cargo.toml
RUN mkdir src \
    && touch src/lib.rs \
    && mkdir brian-node-rust/src \
    && touch brian-node-rust/src/lib.rs \
    && cargo build --release \
    && rm -rf src brian-node-rust/src

ADD src ./src
RUN mkdir brian-node-rust/src \
    && touch brian-node-rust/src/lib.rs \
    && cargo build --release \
    && cargo build --release -p brian-node-rust\
    && rm -rf src brian-node-rust

ADD . ./
RUN cargo build --release -p brian-node-rust
RUN ls -l /brian-node-rust/target/release/brian-node-rust

FROM debian:bullseye-slim
ARG APP=/usr/src/app

RUN apt-get update \
    && apt-get install -y ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*

EXPOSE 8000

ENV TZ=Etc/UTC \
    APP_USER=appuser

RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP}

COPY --from=builder /brian-node-rust/target/release/brian-node-rust ${APP}/brian-node-rust

RUN chown -R $APP_USER:$APP_USER ${APP}

USER $APP_USER
WORKDIR ${APP}

CMD ["./brian-node-rust"]
