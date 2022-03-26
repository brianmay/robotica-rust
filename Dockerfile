FROM rust:1.59-bullseye as builder
RUN USER=root cargo new --bin robotica-node-rust
WORKDIR ./robotica-node-rust

RUN apt-get update \
    && apt-get install -y cmake \
    && rm -rf /var/lib/apt/lists/*

COPY ./Cargo.toml ./Cargo.toml
RUN cargo build --release

ADD . ./
RUN cargo build --release

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

COPY --from=builder /robotica-node-rust/target/release/robotica-node-rust ${APP}/robotica-node-rust

RUN chown -R $APP_USER:$APP_USER ${APP}

USER $APP_USER
WORKDIR ${APP}

CMD ["./robotica-node-rust"]
