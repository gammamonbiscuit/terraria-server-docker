FROM debian:forky-slim AS base

ARG VERSION=latest
ARG TARGETARCH

ENV TERRARIA_VERSION=$VERSION
ENV TERRARIA_DIR=/root/.local/share/Terraria
ENV PATH="${TERRARIA_DIR}:${PATH}"

RUN mkdir -p ${TERRARIA_DIR}

WORKDIR ${TERRARIA_DIR}

COPY ./scripts/* .

RUN chmod +x \
    create-server-config.sh \
    init-TerrariaServer-amd64.sh \
    init-TerrariaServer-arm64.sh \
    download_server.py \
    prune_unused_files.py \
    get_latest_by_iteration.py \
    get_latest_version.py && \
    ln -vs "init-TerrariaServer-${TARGETARCH}.sh" "entrypoint.sh"

RUN apt-get update -qq && apt-get -qq install python3

RUN python3 download_server.py ${TERRARIA_VERSION}

RUN python3 prune_unused_files.py

ENV autocreate=1 \
    seed='' \
    difficulty=1 \
    maxplayers=16 \
    port=7777 \
    password='' \
    motd="Welcome!" \
    worldpath=${TERRARIA_DIR}/Worlds \
    banlist=banlist.txt \
    secure=1 \
    language=en/US \
    upnp=1 \
    npcstream=1 \
    priority=1

RUN mkdir -p ${TERRARIA_DIR}/Worlds

### amd-64 ###

FROM base AS build-amd64

RUN chmod +x TerrariaServer.bin.x86_64

### arm-64 ###

FROM mono:latest AS build-arm64

ENV TERRARIA_DIR=/root/.local/share/Terraria

ENV PATH="${TERRARIA_DIR}:${PATH}" \
    autocreate=1 \
    seed='' \
    difficulty=1 \
    maxplayers=16 \
    port=7777 \
    password='' \
    motd="Welcome!" \
    worldpath=${TERRARIA_DIR}/Worlds \
    banlist=banlist.txt \
    secure=1 \
    language=en/US \
    upnp=1 \
    npcstream=1 \
    priority=1

RUN mkdir -p ${TERRARIA_DIR}

WORKDIR ${TERRARIA_DIR}

COPY --from=base ${TERRARIA_DIR} .

RUN chmod +x TerrariaServer.exe

RUN rm System* Mono* monoconfig mscorlib.dll

FROM build-${TARGETARCH} AS final

ENTRYPOINT [ "./entrypoint.sh" ]