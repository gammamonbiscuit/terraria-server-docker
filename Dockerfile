FROM rust:1.98-alpine AS twall

RUN apk --no-cache --update add build-base
COPY ./twall /twall
WORKDIR /twall
RUN cargo build --release

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
    logging.sh \
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

FROM debian:forky-slim AS build-arm64

ARG DEBIAN_FRONTEND=noninteractive

ENV MONO_VERSION=6.12.0.200

RUN echo "Acquire::http::Proxy \"http://host.docker.internal:3142\";" >> /etc/apt/apt.conf.d/01proxy && \
    echo "Acquire::https::Proxy \"DIRECT\";" >> /etc/apt/apt.conf.d/01proxy && \
    sed -i 's/sha1.second_preimage_resistance.*/sha1.second_preimage_resistance = 2099-12-31/' /usr/share/apt/default-sequoia.config && \
    apt-get update && \
    apt-get install -y --no-install-recommends dirmngr ca-certificates gnupg && \
    gpg --homedir /tmp --no-default-keyring --keyring gnupg-ring:/usr/share/keyrings/mono-official-archive-keyring.gpg --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys 3FA7E0328081BFF6A14DA29AA6A19B38D3D831EF && \
    chmod +r /usr/share/keyrings/mono-official-archive-keyring.gpg && \
    echo "deb [signed-by=/usr/share/keyrings/mono-official-archive-keyring.gpg] https://download.mono-project.com/repo/debian stable-buster/snapshots/${MONO_VERSION} main" | tee /etc/apt/sources.list.d/mono-official-stable.list && \
    apt-get update && \
    apt-get install -y --no-install-recommends mono-runtime binutils curl mono-devel ca-certificates-mono fsharp mono-vbnc nuget referenceassemblies-pcl && \
    rm -rf /var/cache/apt /var/lib/apt /var/cache/debconf /etc/apt/apt.conf.d

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

COPY --chmod=755 --from=twall /twall/target/release/twall ${TERRARIA_DIR}

ENTRYPOINT [ "./entrypoint.sh" ]