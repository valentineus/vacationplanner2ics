# syntax=docker/dockerfile:1
FROM rust:1.98.1-bookworm AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --locked && \
    cp target/release/vacationplanner2ics /usr/local/bin/vacationplanner2ics

FROM python:3.14-slim-bookworm AS test
RUN pip install --no-cache-dir icalendar==7.3.0
COPY --from=build /usr/local/bin/vacationplanner2ics /usr/local/bin/
COPY tests /tests
ENV PYTHONDONTWRITEBYTECODE=1
USER 65532:65532
CMD ["python", "/tests/e2e.py"]

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /usr/local/bin/vacationplanner2ics /usr/local/bin/
USER 65532:65532
EXPOSE 8080
ENTRYPOINT ["vacationplanner2ics"]
