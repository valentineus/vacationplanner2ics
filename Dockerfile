# syntax=docker/dockerfile:1
FROM rust:1.98.1-alpine3.22 AS build
RUN apk add --no-cache cmake make
WORKDIR /build
ARG TARGETARCH
ARG CARGO_BUILD_JOBS=2
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
RUN rustup show active-toolchain
COPY src ./src
RUN --mount=type=cache,id=vacationplanner2ics-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=vacationplanner2ics-target-${TARGETARCH},target=/build/target,sharing=locked \
    cargo fmt --check && \
    cargo clippy --release --locked --all-targets -- -D warnings && \
    cargo build --release --locked && \
    ! readelf -l target/release/vacationplanner2ics | grep -q INTERP && \
    cp target/release/vacationplanner2ics /usr/local/bin/vacationplanner2ics

FROM python:3.14-alpine3.22 AS test
COPY tests/requirements.txt /tests/requirements.txt
RUN pip install --no-cache-dir -r /tests/requirements.txt
COPY --from=build /usr/local/bin/vacationplanner2ics /usr/local/bin/
COPY tests /tests
ENV PYTHONDONTWRITEBYTECODE=1
USER 65532:65532
CMD ["python", "/tests/e2e.py"]

FROM scratch AS runtime
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=build /usr/local/bin/vacationplanner2ics /vacationplanner2ics
LABEL org.opencontainers.image.source="https://code.popov.link/valentineus/vacationplanner2ics" \
      org.opencontainers.image.description="Vacationplanner calendar subscriptions over HTTP" \
      org.opencontainers.image.licenses="MIT"
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["/vacationplanner2ics"]
