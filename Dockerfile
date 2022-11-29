FROM docker.io/library/rust:1.65-bullseye as builder
WORKDIR /brian-backend

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

RUN cargo build --release -p brian-backend
RUN ls -l /brian-backend/target/release/brian-backend
RUN npm -C brian-frontend install
RUN npm -C brian-frontend run build
RUN ls -l /brian-backend/brian-frontend/dist

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

COPY --from=builder /brian-backend/target/release/brian-backend ${APP}/brian-backend
COPY --from=builder /brian-backend/brian-frontend/dist ${APP}/brian-frontend/dist
RUN ls -l ${APP}/brian-frontend/dist

USER $APP_USER
WORKDIR ${APP}

CMD ["./brian-backend"]
