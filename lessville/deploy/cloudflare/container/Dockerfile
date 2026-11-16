# Prepare the exact vendored backend with tools/build_cloudflare_linux.py first.
# Lessville agent service: embedded LessDB, isolated from the parent public engine.
# One instance, five-minute checkpoints, seven-day R2 rollback window; size is in wrangler.jsonc.
# Ubuntu 24.04 (compatible with the Debian 12 build image) with curl, tar, gzip and CA
# certificates preinstalled, so the image needs no RUN step or build-time network.
FROM buildpack-deps:noble-curl@sha256:ff017c6ddb753a820436187146da5035237aca43e0ac53cd64cf5cb18241b733
COPY --chown=10001:10001 data/.keep /data/.keep
COPY release-marker.txt /etc/lessville-release
# The exact tested release binary is split into ordered chunks (one image layer
# each) because single large layer uploads were being cut off on the registry
# path. Chunks are ignored by Git; the tracked marker precedes them so a new
# source checkpoint invalidates every chunk layer. Startup reassembles the
# binary and refuses to run unless it matches the sha256 recorded in the marker.
COPY bin-parts/part-00 /opt/lessville/part-00
COPY bin-parts/part-01 /opt/lessville/part-01
COPY bin-parts/part-02 /opt/lessville/part-02
COPY bin-parts/part-03 /opt/lessville/part-03
COPY bin-parts/part-04 /opt/lessville/part-04
COPY bin-parts/part-05 /opt/lessville/part-05
COPY bin-parts/part-06 /opt/lessville/part-06
COPY bin-parts/part-07 /opt/lessville/part-07
COPY bin-parts/part-08 /opt/lessville/part-08
COPY bin-parts/part-09 /opt/lessville/part-09
COPY bin-parts/part-10 /opt/lessville/part-10
COPY bin-parts/part-11 /opt/lessville/part-11
COPY bin-parts/part-12 /opt/lessville/part-12
COPY bin-parts/part-13 /opt/lessville/part-13
COPY bin-parts/part-14 /opt/lessville/part-14
COPY bin-parts/part-15 /opt/lessville/part-15
COPY bin-parts/part-16 /opt/lessville/part-16
COPY bin-parts/part-17 /opt/lessville/part-17
COPY bin-parts/part-18 /opt/lessville/part-18
COPY bin-parts/part-19 /opt/lessville/part-19
COPY bin-parts/part-20 /opt/lessville/part-20
COPY bin-parts/part-21 /opt/lessville/part-21
COPY bin-parts/part-22 /opt/lessville/part-22
COPY bin-parts/part-23 /opt/lessville/part-23
COPY start.sh /usr/local/bin/lessville-start
USER 10001:10001
ENV LESSVILLE_DATA=/data LESSVILLE_PORT=8790
EXPOSE 8790
CMD ["/bin/sh", "/usr/local/bin/lessville-start"]
