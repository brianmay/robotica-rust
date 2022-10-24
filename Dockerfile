FROM docker.io/library/rust:1.60-bullseye as builder
WORKDIR /brian-node-rust

RUN apt-get update \
    && apt-get install -y cmake \
    && rm -rf /var/lib/apt/lists/*

# Install nodejs
RUN curl -sL https://deb.nodesource.com/setup_16.x | bash -
RUN apt-get update && apt-get install nodejs

# Install wasm-pack
RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

ADD ./ ./
ARG BUILD_DATE=date
ARG VCS_REF=vcs
ENV BUILD_DATE=${BUILD_DATE}
ENV VCS_REF=${VCS_REF}

RUN echo "pub const BUILD_DATE: &str = \"$BUILD_DATE\";" > robotica-frontend/src/version.rs
RUN echo "pub const VCS_REF: &str = \"$VCS_REF\";" >> robotica-frontend/src/version.rs
RUN cat robotica-frontend/src/version.rs

RUN cargo build --release -p brian-node-rust
RUN ls -l /brian-node-rust/target/release/brian-node-rust
RUN npm -C robotica-frontend install
RUN npm -C robotica-frontend run build
RUN ls -l /brian-node-rust/robotica-frontend/dist

FROM debian:bullseye-slim
ARG APP=/usr/src/app

ARG BUILD_DATE=date
ARG VCS_REF=vcs
ENV BUILD_DATE=${BUILD_DATE}
ENV VCS_REF=${VCS_REF}

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
COPY --from=builder /brian-node-rust/robotica-frontend/dist ${APP}/robotica-frontend/dist
RUN ls -l ${APP}/robotica-frontend/dist

RUN chown -R $APP_USER:$APP_USER ${APP}

USER $APP_USER
WORKDIR ${APP}

CMD ["./brian-node-rust"]
