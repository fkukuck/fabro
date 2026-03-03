ARG PYTHON_VERSION=3.11
FROM python:${PYTHON_VERSION}-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    git \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -s /bin/bash developer
WORKDIR /workspaces/app
