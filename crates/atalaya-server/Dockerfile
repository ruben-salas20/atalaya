# Imagen del servicio. Se construye desde la raíz del repo:
#   docker build -f crates/atalaya-server/Dockerfile .
FROM rust:1.98-slim-trixie AS build
WORKDIR /src
COPY . .
# Las cachés guardan las dependencias ya compiladas entre construcciones.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p atalaya-server \
    && cp target/release/atalaya-server /usr/local/bin/

# MathJax para las fórmulas de los papers completos (ADR 0010). Solo hace
# falta el binario de node y los módulos: se copian a la imagen final.
FROM node:22-trixie-slim AS mathjax
WORKDIR /opt/mathjax
COPY crates/atalaya-server/mathjax/package.json crates/atalaya-server/mathjax/package-lock.json ./
RUN npm ci --omit=dev --no-audit --no-fund
COPY crates/atalaya-server/mathjax/svg.mjs ./

FROM debian:trixie-slim
# sqlite3 sirve a la copia diaria: hace un volcado coherente con `.backup`.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates sqlite3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -u 1000 -m atalaya \
    && mkdir /data && chown atalaya:atalaya /data
COPY --from=build /usr/local/bin/atalaya-server /usr/local/bin/
COPY --from=mathjax /usr/local/bin/node /usr/local/bin/node
COPY --from=mathjax /opt/mathjax /opt/mathjax
USER atalaya
VOLUME /data
EXPOSE 8080
CMD ["atalaya-server"]
