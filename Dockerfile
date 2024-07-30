FROM docker.io/library/rust:1.71-bullseye as builder

RUN apt-get update \
    && apt-get install -y cmake protobuf-compiler python3-dev ca-certificates curl gnupg python3-pip \
    && mkdir -p /etc/apt/keyrings \
    && curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key \
     | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg \
    && NODE_MAJOR=16 \
    && echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_$NODE_MAJOR.x nodistro main" \
     > /etc/apt/sources.list.d/nodesource.list \
    && apt-get update \
    && apt-get install nodejs -y \
    && rm -rf /var/lib/apt/lists/*

# Install wasm-pack
RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

WORKDIR /python
RUN pip install poetry==1.6.1
ENV POETRY_NO_INTERACTION=1 \
    POETRY_VIRTUALENVS_IN_PROJECT=1 \
    POETRY_VIRTUALENVS_CREATE=1 \
    POETRY_CACHE_DIR=/tmp/poetry_cache
COPY robotica-tokio/python/pyproject.toml robotica-tokio/python/poetry.lock ./
RUN touch README.md
RUN --mount=type=cache,target=$POETRY_CACHE_DIR poetry install --without dev --no-root

ARG BUILD_DATE=date
ARG VCS_REF=vcs
ENV BUILD_DATE=${BUILD_DATE}
ENV VCS_REF=${VCS_REF}

WORKDIR /brian-backend
ADD ./ ./
RUN cargo build --release -p brian-backend
RUN ls -l /brian-backend/target/release/brian-backend
RUN npm -C robotica-frontend install
RUN npm -C robotica-frontend run build
RUN ls -l /brian-backend/robotica-frontend/dist

FROM debian:bullseye-slim
ARG APP=/usr/src/app

ARG BUILD_DATE=date
ARG VCS_REF=vcs
ENV BUILD_DATE=${BUILD_DATE}
ENV VCS_REF=${VCS_REF}

RUN apt-get update \
    && apt-get install -y ca-certificates tzdata python3 libpython3.9 \
    && rm -rf /var/lib/apt/lists/*

EXPOSE 8000

ENV TZ=Etc/UTC \
    APP_USER=appuser

RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP}

WORKDIR ${APP}

COPY --from=builder /python/.venv ${APP}/.venv
COPY --from=builder /brian-backend/target/release/brian-backend ${APP}/brian-backend
COPY --from=builder /brian-backend/robotica-frontend/dist ${APP}/robotica-frontend/dist
RUN ls -l ${APP}/robotica-frontend/dist

ENV PATH="${APP}/.venv/bin:$PATH"
USER $APP_USER

CMD [ "./brian-backend" ]
